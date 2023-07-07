use crate::alloc::HeapRef;
use crate::container::{Arc, Weak, Vec};
use crate::sync::IMutex;
use crate::prelude::*;

use super::Thread;

/// This stores all of the ready threads, used by scheduler to pick next thread
#[derive(Debug)]
pub struct ThreadMap {
    // TODO: use a better data structure than a vec
    ready_threads: IMutex<Vec<Weak<Thread>>>,
}

impl ThreadMap {
    pub const fn new(allocer: HeapRef) -> Self {
        ThreadMap {
            ready_threads: IMutex::new(Vec::new(allocer)),
        }
    }

    /// Gets the next thread to run
    /// 
    /// Returns `None` if there are no available threads to run
    pub fn get_ready_thread(&self) -> Option<Arc<Thread>> {
        let mut ready_threads = self.ready_threads.lock();

        loop {
            let thread = ready_threads.pop_front()?;
            if let Some(thread) = thread.upgrade() {
                return Some(thread);
            }
        }
    }

    /// Adds `thread` to the list of ready threads
    pub fn insert_ready_thread(&self, thread: Weak<Thread>) -> KResult<()> {
        self.ready_threads.lock().push(thread)
    }
}