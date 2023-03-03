use core::sync::atomic::{AtomicUsize, AtomicBool};

use spin::Once;

use crate::container::Arc;
use crate::sched::Thread;
use crate::alloc::{PaRef, OrigRef, root_alloc_page_ref, root_alloc_ref};
use crate::cap::{CapFlags, CapObject, StrongCapability, WeakCapability};
use crate::prelude::*;
use crate::sync::IMutex;

mod vmem_manager;
pub use vmem_manager::{VirtAddrSpace, PageMappingFlags};

#[derive(Debug)]
pub struct Process {
    pub is_alive: AtomicBool,
    pub num_threads_running: AtomicUsize,
    strong_capability: IMutex<Option<StrongCapability<Self>>>,
    name: String,
    threads: Vec<Arc<Thread>>,
    addr_space: VirtAddrSpace,
}

impl Process {
    pub fn new(page_allocator: PaRef, allocer: OrigRef, name: String) -> KResult<WeakCapability<Self>> {
        let strong_cap = StrongCapability::new(
            Process {
                is_alive: AtomicBool::new(true),
                num_threads_running: AtomicUsize::new(0),
                strong_capability: IMutex::new(None),
                name,
                threads: Vec::new(allocer.clone().downgrade()),
                addr_space: VirtAddrSpace::new(page_allocator, allocer.downgrade())?,
            },
            CapFlags::READ | CapFlags::PROD | CapFlags::WRITE,
            allocer,
        )?;

        *strong_cap.object().strong_capability.lock() = Some(strong_cap.clone());

        Ok(strong_cap.downgrade())
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
}

impl CapObject for Process {
    fn cap_drop(&self) {}
}

static KERNEL_PROCESS: Once<WeakCapability<Process>> = Once::new();

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
    });
}

/// Gets the kernel process, and panics if it has not yet been initialized
pub fn get_kernel_process() -> WeakCapability<Process> {
    KERNEL_PROCESS.get().unwrap().clone()
}