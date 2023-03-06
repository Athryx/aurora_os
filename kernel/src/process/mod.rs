use core::sync::atomic::{AtomicUsize, AtomicBool, Ordering};
use core::slice;

use spin::Once;

use crate::arch::x64::{asm_thread_init, IntDisable};
use crate::container::{Arc, Weak};
use crate::int::IPI_PROCESS_EXIT;
use crate::int::apic::{Ipi, IpiDest};
use crate::mem::MemOwner;
use crate::sched::{Tid, Thread, ThreadHandle, ThreadState, PostSwitchAction, THREAD_MAP, switch_current_thread_to};
use crate::alloc::{PaRef, OrigRef, root_alloc_page_ref, root_alloc_ref};
use crate::cap::{CapFlags, CapObject, StrongCapability, WeakCapability, CapabilityMap, CapType};
use crate::prelude::*;
use crate::sched::kernel_stack::KernelStack;
use crate::sync::IMutex;

mod spawner;
pub use spawner::Spawner;
mod vmem_manager;
pub use vmem_manager::{VirtAddrSpace, PageMappingFlags};

/// Passed to create_thread to specify which state thread should start in
#[derive(Debug, Clone, Copy)]
pub enum ThreadStartMode {
    Ready,
    Suspended,
}

#[derive(Debug)]
pub struct Process {
    name: String,

    page_allocator: PaRef,
    heap_allocator: OrigRef,

    pub is_alive: AtomicBool,
    pub num_threads_running: AtomicUsize,

    strong_capability: IMutex<Option<StrongCapability<Self>>>,
    self_weak: Once<Weak<Self>>,

    /// Counter used to assign thread ids
    next_tid: AtomicUsize,
    threads: IMutex<Vec<Arc<Thread>>>,

    addr_space: VirtAddrSpace,
    cap_map: CapabilityMap,
}

impl Process {
    pub fn new(page_allocator: PaRef, allocer: OrigRef, name: String) -> KResult<WeakCapability<Self>> {
        let strong_cap = StrongCapability::new(
            Process {
                name,
                page_allocator: page_allocator.clone(),
                heap_allocator: allocer.clone(),
                is_alive: AtomicBool::new(true),
                num_threads_running: AtomicUsize::new(0),
                strong_capability: IMutex::new(None),
                self_weak: Once::new(),
                next_tid: AtomicUsize::new(0),
                threads: IMutex::new(Vec::new(allocer.clone().downgrade())),
                addr_space: VirtAddrSpace::new(page_allocator, allocer.clone().downgrade())?,
                cap_map: CapabilityMap::new(allocer.downgrade()),
            },
            CapFlags::READ | CapFlags::PROD | CapFlags::WRITE,
            allocer,
        )?;

        *strong_cap.object().strong_capability.lock() = Some(strong_cap.clone());
        strong_cap.object().self_weak.call_once(|| Arc::downgrade(strong_cap.inner()));

        Ok(strong_cap.downgrade())
    }
    
    pub fn page_allocator(&self) -> PaRef {
        self.page_allocator.clone()
    }

    pub fn heap_allocator(&self) -> OrigRef {
        self.heap_allocator.clone()
    }

    pub fn self_weak(&self) -> Weak<Self> {
        self.self_weak.get().unwrap().clone()
    }

    /// Returns the value that should be loaded in the cr3 register
    /// 
    /// This is the pointer to the top lavel paging table for the process
    pub fn get_cr3(&self) -> usize {
        self.addr_space.get_cr3_addr().as_usize()
    }

    /// Releases the strong capbility for the process, which will lead to the process being destroyed
    /// 
    /// # Safety
    /// 
    /// Don't do this with any of the process' threads running
    pub unsafe fn release_strong_capability(&self) {
        *self.strong_capability.lock() = None;
    }

    /// Gets a unique valid Tid
    fn next_tid(&self) -> Tid {
        Tid::from(self.next_tid.fetch_add(1, Ordering::Relaxed))
    }

    /// Inserts the thread into the thread list
    /// 
    /// The thread list is sorted by Tid
    fn insert_thread(&self, thread: Arc<Thread>) -> KResult<()> {
        let mut thread_list = self.threads.lock();

        let insert_index = thread_list
            .binary_search_by_key(&thread.tid, |thread| thread.tid)
            .expect_err("duplicate tids detected");

        thread_list.insert(insert_index, thread)
    }

    /// Crates a new idle thread structure for the currently running thread
    /// 
    /// `stack` should be a virt range referencing the whole stack of the current thread
    pub fn create_idle_thread(&self, name: String, stack: AVirtRange) -> KResult<(Arc<Thread>, MemOwner<ThreadHandle>)> {
        let (thread, thread_handle) = Thread::new(
            self.next_tid(),
            name,
            self.self_weak(),
            KernelStack::Existing(stack),
            // rsp will be set on thread switch, so it can be 0 for now
            0,
        )?;

        self.insert_thread(thread.clone())?;

        Ok((thread, thread_handle))
    }

    /// Creates a new thread
    /// 
    /// The thread will return to userspace code at rip upon starting
    /// 
    /// Rsp will be initialized, as well as 4 general purpose registers
    pub fn create_thread(
        &self,
        name: String,
        start_mode: ThreadStartMode,
        rip: usize,
        rsp: usize,
        regs: (usize, usize, usize, usize)
    ) -> KResult<Tid> {
        let kernel_stack = KernelStack::new(self.page_allocator())?;

        // safety: kernel_stack points to valid memory
        let stack_slice = unsafe { 
            slice::from_raw_parts_mut(
                kernel_stack.stack_base().as_mut_ptr(),
                kernel_stack.as_virt_range().size() / size_of::<usize>(),
            )
        };

        let mut push_index = 0;
        let mut push = |val: usize| {
            stack_slice[stack_slice.len() - 1 -push_index] = val;
            push_index += 1;
        };

        // setup stack the first thing the new thread does is
        // load the specified registers and jump to userspace code
        push(rsp);
        push(rip);
        push(regs.3);
        push(regs.2);
        push(regs.1);
        push(regs.0);
        push(asm_thread_init as usize);
        push(0);
        push(0);
        push(0);
        push(0);
        push(0);
        push(0);
        push(0x202);

        let tid = self.next_tid();
        let kernel_rsp = kernel_stack.stack_top() - 8 * push_index;
        let (thread, thread_handle) = Thread::new(
            tid,
            name,
            self.self_weak(),
            kernel_stack,
            kernel_rsp.as_usize(),
        )?;

        self.insert_thread(thread)?;

        // insert thread handle into scheduler after all other setup is done
        match start_mode {
            ThreadStartMode::Ready => THREAD_MAP.insert_ready_thread(thread_handle),
            ThreadStartMode::Suspended => THREAD_MAP.insert_suspended_thread(thread_handle),
        }

        Ok(tid)
    }

    pub fn is_current_process(&self) -> bool {
        let current_addr = cpu_local_data().current_process_addr.load(Ordering::Acquire);
        current_addr == self as *const _ as usize
    }

    /// Trigger the process to exit
    /// 
    /// This will stop all running threads from this process, and drop the process eventually
    /// 
    /// This may trigger the current thread to die if it is exiting its own process,
    /// so no locks or refcounted objects hould be held when calling this,
    /// unless it has already been checked that `this` is not the current process
    /// 
    /// # Locking
    /// 
    /// acquires local_apic lock
    pub fn exit(this: Arc<Process>) {
        if !this.is_alive.swap(false, Ordering::AcqRel) {
            // another thread is already terminating this process
            return
        }

        cpu_local_data().local_apic().send_ipi(Ipi::To(IpiDest::AllExcludeThis, IPI_PROCESS_EXIT));

        if this.is_current_process() {
            // wait for all other threads except this one to exit
            while this.num_threads_running.load(Ordering::Acquire) != 1 {}

            drop(this);

            switch_current_thread_to(
                ThreadState::Dead { try_destroy_process: true },
                // creating a new int disable is fine, we don't care to restore interrupts because this thread will die
                IntDisable::new(),
                PostSwitchAction::None
            ).unwrap();
        } else {
            // wait for all other threads to exit
            while this.num_threads_running.load(Ordering::Acquire) != 0 {}

            // safety: no other threads from this process are running
            unsafe {
                this.release_strong_capability();
            }
        }
    }
}

impl CapObject for Process {
    const TYPE: CapType = CapType::Process;
}

static KERNEL_PROCESS: Once<Arc<Process>> = Once::new();

/// Initializes the kernel process
pub fn init_kernel_process() {
    const FAIL_MESSAGE: &'static str = "could not initialize kernel process";

    KERNEL_PROCESS.call_once(|| {
        Process::new(
            root_alloc_page_ref(),
            root_alloc_ref(),
            String::from_str(root_alloc_ref().downgrade(), "kernel")
                .expect(FAIL_MESSAGE)
        ).expect(FAIL_MESSAGE)
            .inner()
            .upgrade()
            .expect(FAIL_MESSAGE)
    });
}

/// Gets the kernel process, and panics if it has not yet been initialized
/// 
/// The kernel process just has all the idle threads for each cpu
pub fn get_kernel_process() -> Arc<Process> {
    KERNEL_PROCESS.get().unwrap().clone()
}