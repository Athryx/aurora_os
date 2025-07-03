use core::cmp::{Ordering, Reverse};

use sys::KResult;

use crate::{container::BinaryHeap, mem::HeapRef};
use super::{ThreadRef, thread::WakeReason};

#[derive(Debug, Clone)]
struct ThreadTimeout {
    timeout_nsec: u64,
    thread: ThreadRef,
}

impl PartialEq for ThreadTimeout {
    fn eq(&self, other: &Self) -> bool {
        self.timeout_nsec == other.timeout_nsec
    }
}

impl Eq for ThreadTimeout {}

impl PartialOrd for ThreadTimeout {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ThreadTimeout {
    fn cmp(&self, other: &Self) -> Ordering {
        self.timeout_nsec.cmp(&other.timeout_nsec)
    }
}

#[derive(Debug)]
pub struct TimeoutQueue {
    threads: BinaryHeap<Reverse<ThreadTimeout>>,
}

impl TimeoutQueue {
    pub fn new(allocator: HeapRef) -> Self {
        TimeoutQueue {
            threads: BinaryHeap::new(allocator),
        }
    }

    /// Wakes all threads that are scheduled to wake up before `current_nsec`
    pub fn wake_threads(&mut self, current_nsec: u64) {
        while let Some(next_thread) = self.threads.peek() {
            if next_thread.0.timeout_nsec <= current_nsec {
                // panic safety: peek already checked that this exists
                let Reverse(next_thread) = self.threads.pop().unwrap();

                next_thread.thread.move_to_ready_list(WakeReason::Timeout);
            } else {
                break;
            }
        }
    }

    pub fn insert_thread(&mut self, thread: ThreadRef, timeout_nsec: u64) -> KResult<()> {
        self.threads.push(Reverse(ThreadTimeout {
            timeout_nsec,
            thread,
        }))
    }
}