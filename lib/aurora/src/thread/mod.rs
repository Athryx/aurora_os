use core::arch::asm;
use core::sync::atomic::{Ordering, AtomicU8, AtomicU64};
use alloc::{sync::Arc, string::String};

use sys::syscall_nums::{MEMORY_UNMAP, THREAD_DESTROY};
use sys::{CapId, Capability, Thread as SysThread};

mod thread_local_data;
pub use thread_local_data::{LocalKey, ThreadLocalData};

use crate::{process, addr_space, this_context};

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

// start at 1 for the initial thread
static NUM_THREADS: AtomicU64 = AtomicU64::new(1);

/// Exits the calling thread
/// 
/// This function should not normally be used, it is public only for std to call when main thread exits
pub fn exit() -> ! {
    // this is a thread local variable, must call before deallocating thread local data
    let stack_address = current().0.stack_region_address;

    // safety: thread local data is assumed to be initialized, and it is no longer use beyond this point
    unsafe {
        ThreadLocalData::dealloc();
    }
    
    if NUM_THREADS.fetch_sub(1, Ordering::Relaxed) == 1 {
        // we are the last thread exiting, exit process
        process::exit();
    } else {
        let transient_pointer = addr_space().unmap_transient(stack_address)
            .expect("failed to transiently unmap stack address")
            .expect("failed to transiently unmap stack address");

        let address_space_id = this_context().address_space.as_usize();

        thread_exit_asm(MEMORY_UNMAP, address_space_id, stack_address, transient_pointer, THREAD_DESTROY);
    }
}

#[naked]
extern "C" fn thread_exit_asm(
    unmap_syscall_num: u32,
    address_space_id: usize,
    stack_address: usize,
    transient_pointer: *const AtomicU64,
    thread_exit_syscall_num: u32,
) -> ! {
    unsafe {
        asm!(
            "mov eax, edi", // mov unmap syscall num to eax
            "mov rbx, rsi", // mov address space id to syscall arg 1
            "mov r9, rcx", // save transient pointer for later, r9 is saved register for syscalls
            "syscall", // isssue unmap syscall
            // TODO: make sure this has release semantics
            "lock dec qword ptr [r9]", // decrament the transient counter now that stack is unmapped
            "mov eax, r8d", // move thread exit syscall to eax, this zeroes upper bits of eax, which is the flag for exit self
            "syscall", // issue thread destroy syscall to exit the current thread
            options(noreturn),
        )
    }
}