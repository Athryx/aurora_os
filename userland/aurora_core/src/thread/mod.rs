use core::arch::asm;
use core::sync::atomic::{Ordering, AtomicU8, AtomicU64};
use core::mem::size_of;
use core::ptr;
use alloc::{sync::Arc, string::String};

use sys::syscall_nums::{ADDRESS_SPACE_UNMAP, THREAD_DESTROY};
use sys::{CapId, Capability, Thread as SysThread, SysErr, MemoryMappingOptions};

mod thread_local_data;
pub use thread_local_data::{LocalKey, ThreadLocalData};

use crate::prelude::*;
use crate::allocator::addr_space::{MapMemoryArgs, MapMemoryResult};
use crate::sync::Mutex;
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

/// An owned permission to join on a thread (block on its termination)
pub struct JoinHandle<T> {
    thread: Thread,
    /// Returns value of thread
    /// 
    // TODO: make this into just unsafe cell, technically mutex is not needed because thread join synchronizes writes
    result: Arc<Mutex<Option<T>>>,
}

impl<T> JoinHandle<T> {
    /// Extracts a handle to the underlying thread
    pub fn thread(&self) -> &Thread {
        &self.thread
    }

    /// Waits for the associated thread to finish
    /// 
    /// This function will return immediately if the associated thread has already finished
    pub fn join(self) -> T {
        match self.thread.0.thread.handle_thread_exit_sync(None) {
            // thread has exited
            Ok(_) => (),
            // the thread id was not valid, which at this point means the thread already exited
            // TODO: stop thread later being dropped, that is just an extra syscall for nothing
            Err(SysErr::InvlId) => (),
            Err(_) => panic!("could not join on thread"),
        }

        self.result.lock().take().expect("thread join did not return value")
    }

    /// Checks if the associated thread has finished running its main function
    pub fn is_finished(&self) -> bool {
        // if the refcount is only 1, the other thread has dropped its result
        // this is ok to check this way because real std library does the same,
        // and it only guarentees the rust closure has stopped running
        Arc::strong_count(&self.result) == 1
    }
}

pub fn spawn<F, T>(f: F) -> JoinHandle<T>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static {
    
    let MapMemoryResult {
        address,
        size,
        ..
    } = addr_space().map_memory(MapMemoryArgs {
        size: Some(process::DEFAULT_STACK_SIZE),
        options: MemoryMappingOptions {
            read: true,
            write: true,
            ..Default::default()
        },
        ..Default::default()
    }).expect("failed to map new thread stack");

    // there will be 1 pointer on the stack
    let rsp = address + size.bytes() - size_of::<usize>();

    let context = this_context();
    let sys_thread = SysThread::new(
        &context.allocator,
        &context.thread_group,
        &context.address_space,
        &context.capability_space,
        thread_spawn_asm as usize,
        rsp,
        sys::ThreadStartMode::Suspended,
    ).expect("failed to spawn thread");

    let thread = Thread::new(None, sys_thread, address);
    let join_result = Arc::new(Mutex::new(None));

    let joind_handle = JoinHandle {
        thread: thread.clone(),
        result: join_result.clone(),
    };

    let closure = move || {
        let result = f();
        *join_result.lock() = Some(result);
    };

    let startup_data = Box::new(ThreadStartupData {
        thread: thread.clone(),
        closure: Box::new(closure),
    });

    let startup_data_ptr = Box::leak(startup_data) as *mut _;

    // write startup data and startup fn pointer to stack
    let stack_ptr = rsp as *mut usize;
    unsafe {
        ptr::write(stack_ptr, startup_data_ptr as usize);
    }

    // start the thread now
    thread.0.thread.resume().expect("failed to start thread");

    joind_handle
}

struct ThreadStartupData {
    thread: Thread,
    closure: Box<dyn FnOnce()>,
}

#[no_mangle]
unsafe extern "C" fn thread_startup(data: *mut ThreadStartupData) -> ! {
    {
        let ThreadStartupData {
            thread,
            closure,
        } = unsafe { *Box::from_raw(data) };

        ThreadLocalData::init(thread);

        // run thread function and report result to join
        closure();
    }

    // exit will take care of dropping thread local data and deallocating stack
    // everything else is dropepd by now
    exit();
}

#[naked]
unsafe extern "C" fn thread_spawn_asm() -> ! {
    unsafe {
        asm!(
            "pop rdi", // get pointer to startup data
            "call thread_startup", // call startup function, stack should be 16 byte aligned at this point
            options(noreturn),
        )
    }
}

// start at 1 for the initial thread
static NUM_THREADS: AtomicU64 = AtomicU64::new(1);

/// Exits the calling thread
/// 
/// This function should not normally be used, it is public only for std to call when main thread exits
pub fn exit() -> ! {
    if NUM_THREADS.fetch_sub(1, Ordering::Relaxed) == 1 {
        // safety: thread local data is assumed to be initialized, and it is no longer use beyond this point
        unsafe {
            ThreadLocalData::dealloc();
        }

        // we are the last thread exiting, exit process
        process::exit();
    } else {
        exit_thread_only();
    }
}

/// Exits the calling thread
/// 
/// Will not exit the thread group, even if this is the last thread
pub fn exit_thread_only() -> ! {
    // this is a thread local variable, must call before deallocating thread local data
    let stack_address = current().0.stack_region_address;

    let transient_pointer = addr_space().unmap_transient(stack_address)
        .expect("failed to transiently unmap stack address")
        .expect("failed to transiently unmap stack address");

    let address_space_id = this_context().address_space.as_usize();

    // safety: thread local data is assumed to be initialized, and it is no longer use beyond this point
    unsafe {
        ThreadLocalData::dealloc();
    }

    thread_exit_asm(ADDRESS_SPACE_UNMAP, address_space_id, stack_address, transient_pointer, THREAD_DESTROY);
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