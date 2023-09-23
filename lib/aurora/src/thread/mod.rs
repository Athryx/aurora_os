use core::sync::atomic::{Ordering, AtomicU8};
use alloc::{sync::Arc, string::String};

use sys::{CapId, Capability, Thread as SysThread};

mod thread_local_data;
pub use thread_local_data::{LocalKey, ThreadLocalData};

/// An opaque, unique identifier for a thread
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ThreadId(CapId);

/// This is the data stored inside of the Thread type
#[repr(C)]
#[derive(Debug)]
struct ThreadInner {
    name: Option<String>,
    thread: SysThread,
    park_status: AtomicU8,
    /// The address to the start of the stack memory region for this thread
    stack_region_address: usize,
}

/// A handle to a thread
#[derive(Debug, Clone)]
pub struct Thread(Arc<ThreadInner>);

impl Thread {
    pub(crate) fn new(name: Option<String>, sys_thread: SysThread, stack_region_address: usize) -> Self {
        let inner = Arc::new(ThreadInner {
            name,
            thread: sys_thread,
            park_status: AtomicU8::new(0),
            stack_region_address,
        });

        Thread(inner)
    }

    /// Gets the thread's unique identifier
    pub fn id(&self) -> ThreadId {
        ThreadId(self.0.thread.cap_id())
    }

    /// Gets the thread's name
    pub fn name(&self) -> Option<&str> {
        self.0.name.as_deref()
    }
}

/// Gets a handle to the thread that invokes it
pub fn current() -> Thread {
    // FIXME: this is technically unsafe as thread local data may not be initialized
    let local_data = unsafe {
        ThreadLocalData::get().as_ref().unwrap()
    };

    local_data.thread.clone()
}

/// Cooperatively gives up the calling threads timeslice to the OS scheduler
pub fn yield_now() {
    sys::Thread::yield_current();
}