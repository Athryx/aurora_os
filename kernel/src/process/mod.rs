use core::sync::atomic::{AtomicUsize, Ordering, AtomicBool};

use crate::container::Arc;
use crate::sched::Thread;
use crate::alloc::{OrigRef, root_alloc_page_ref};
use crate::cap::{CapFlags, CapObject, StrongCapability};
use crate::prelude::*;

mod vmem_manager;
pub use vmem_manager::{VirtAddrSpace, PageMappingFlags};

#[derive(Debug)]
pub struct Process {
    pub is_alive: AtomicBool,
    pub num_threads_running: AtomicUsize,
    threads: Vec<Arc<Thread>>,
}

impl Process {
    pub fn new(allocer: OrigRef) -> KResult<StrongCapability<Self>> {
        StrongCapability::new(
            Process {
                is_alive: AtomicBool::new(true),
                num_threads_running: AtomicUsize::new(0),
                threads: Vec::new(allocer.clone().downgrade()),
            },
            CapFlags::READ | CapFlags::PROD | CapFlags::WRITE,
            allocer,
        )
    }

    /// Returns the value that should be loaded in the cr3 register
    /// 
    /// This is the pointer to the top lavel paging table for the process
    pub fn get_cr3(&self) -> usize {
        todo!();
    }
}

impl CapObject for Process {
    fn cap_drop(&self) {}
}
