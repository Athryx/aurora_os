use alloc::sync::{Arc, Weak};
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
    pub const fn new() -> Self {
        ThreadMap {
            ready_threads: IMutex::new(Vec::new()),
        }
    }

    /// Gets the next thread and process to run
    /// 
    /// Returns `None` if there are no available threads to run
    /// Also removes any dead threads that are encountered from the ready threads list
    pub fn get_next_thread(&self) -> Option<Arc<Thread>> {
        let mut ready_threads = self.ready_threads.lock();

        loop {
            let thread = ready_threads.pop_front()?;
            let Some(thread) = thread.upgrade() else {
                continue;
            };

            if !thread.is_alive() {
                continue;
            }

            return Some(thread);
        }
    }

    /// Adds `thread` to the list of ready threads
    pub fn insert_ready_thread(&self, thread: Weak<Thread>) -> KResult<()> {
        self.ready_threads.lock().push(thread)
    }
}