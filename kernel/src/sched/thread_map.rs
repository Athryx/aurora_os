use core::sync::atomic::AtomicPtr;

use super::thread::ThreadHandle;
use crate::alloc::root_alloc_ref;
use crate::container::{LinkedList, Vec};
use crate::gs_data::prid;
use crate::sync::IMutex;
use crate::prelude::*;

/// This stores all currently non running threads
#[derive(Debug)]
pub struct ThreadMap {
    ready_threads: IMutex<LinkedList<ThreadHandle>>,
    suspended_threads: IMutex<LinkedList<ThreadHandle>>,
    suspended_timeout_threads: IMutex<LinkedList<ThreadHandle>>,
}

impl ThreadMap {
    pub const fn new() -> Self {
        ThreadMap {
            ready_threads: IMutex::new(LinkedList::new()),
            suspended_threads: IMutex::new(LinkedList::new()),
            suspended_timeout_threads: IMutex::new(LinkedList::new()),
        }
    }
}

unsafe impl Send for ThreadMap {}
unsafe impl Sync for ThreadMap {}