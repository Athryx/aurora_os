use crate::container::LinkedList;
use crate::container::Vec;
use crate::alloc::root_alloc_ref;
use crate::gs_data::prid;
use super::thread::Thread;

#[derive(Debug)]
pub struct ThreadMap {
    // this is a vector where each linked list will correspond to a given cpu
    // the linked list will only ever have 1 thread running at a time,
    // it is just used so the threads can remove themselves from a linked list whenever they change state
    current_thread: Vec<LinkedList<Thread>>,
    ready_threads: LinkedList<Thread>,
    dead_threads: LinkedList<Thread>,
}

impl ThreadMap {
    pub fn new() -> Self {
        ThreadMap {
            current_thread: Vec::new(root_alloc_ref().downgrade()),
            ready_threads: LinkedList::new(),
            dead_threads: LinkedList::new(),
        }
    }

    pub fn get_current_thread(&self) -> &Thread {
        &self.current_thread[prid().into()][0]
    }

    pub fn get_current_thread_mut(&mut self) -> &mut Thread {
        &mut self.current_thread[prid().into()][0]
    }

    // each cpu will call this function to make sure there are enough elments in each vector
    // that stores a cpu local data structur in the thread map
    pub fn ensure_cpu(&mut self) {
        self.current_thread.push(LinkedList::new()).expect("could not set up thread map cpu local data");
    }
}