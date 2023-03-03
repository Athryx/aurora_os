use core::sync::atomic::AtomicPtr;

use super::thread::ThreadHandle;
use crate::alloc::root_alloc_ref;
use crate::container::{LinkedList, Vec};
use crate::gs_data::prid;
use crate::mem::MemOwner;
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

    /// Gets the next thread to run
    /// 
    /// Returns `None` if there are no available threads to run
    pub fn get_ready_thread(&self) -> Option<MemOwner<ThreadHandle>> {
        self.ready_threads.lock().pop_front()
    }

    /// Adds `thread_handle` to the list of ready threads
    pub fn insert_ready_thread(&self, thread_handle: MemOwner<ThreadHandle>) {
        self.ready_threads.lock().push(thread_handle);
    }

    /// Adds `thread_handle` to the list of suspended threads
    pub fn insert_suspended_thread(&self, thread_handle: MemOwner<ThreadHandle>) {
        self.suspended_threads.lock().push(thread_handle);
    }

    /// Adds `thread_handle` to the list of suspended timeout threads
    pub fn insert_suspended_timeout_thread(&self, thread_handle: MemOwner<ThreadHandle>) {
        self.suspended_timeout_threads.lock().push(thread_handle);
    }
}

unsafe impl Send for ThreadMap {}
unsafe impl Sync for ThreadMap {}