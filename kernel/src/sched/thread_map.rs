use core::sync::atomic::AtomicPtr;

use super::thread::ThreadHandle;
use crate::alloc::root_alloc_ref;
use crate::container::{LinkedList, Vec};
use crate::gs_data::prid;
use crate::prelude::*;

#[derive(Debug)]
pub struct ThreadMap {
    /// Each element corresponds to a thread running on a given cpu
    current_thread: Vec<AtomicPtr<ThreadHandle>>,
    ready_threads: LinkedList<ThreadHandle>,
    suspended_threads: LinkedList<ThreadHandle>,
    suspended_timeout_threads: LinkedList<ThreadHandle>,
}

impl ThreadMap {
    pub fn new() -> Self {
        ThreadMap {
            current_thread: Vec::new(root_alloc_ref().downgrade()),
            ready_threads: LinkedList::new(),
            suspended_threads: LinkedList::new(),
            suspended_timeout_threads: LinkedList::new(),
        }
    }

    // each cpu will call this function to make sure there are enough elments in each vector
    // that stores a cpu local data structure in the thread map
    pub fn ensure_cpu(&mut self) -> KResult<()> {
        self.current_thread.push(AtomicPtr::new(null_mut()))
    }
}

unsafe impl Send for ThreadMap {}