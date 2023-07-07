use core::sync::atomic::{AtomicUsize, Ordering};

use crate::container::Arc;
use super::kernel_stack::KernelStack;
use crate::container::Weak;
use crate::process::Process;
use crate::prelude::*;

pub use sys::Tid;

/// Amount status must be incramented to change generation without changing ThreadState
const GENERATION_STEP_SIZE: usize = 0b100;

const THREAD_STATE_MASK: usize = 0b11;

#[repr(u8)]
#[derive(Debug, Clone, Copy)]
pub enum ThreadState {
    Running = 0,
    Ready = 1,
    Suspended = 2,
    Dead = 3,
}

impl ThreadState {
    pub fn from_usize(n: usize) -> Self {
        match n & THREAD_STATE_MASK {
            0 => ThreadState::Running,
            1 => ThreadState::Ready,
            2 => ThreadState::Suspended,
            3 => ThreadState::Dead,
            _ => unreachable!(),
        }
    }

    // Converts the thread state to a thread status, preserves the generation number of old status
    pub fn to_status(&self, old_status: usize) -> usize {
        (old_status & !THREAD_STATE_MASK) | *self as usize
    }
}

#[derive(Debug)]
pub struct Thread {
    pub tid: Tid,
    name: String,
    status: AtomicUsize,
    pub process: Weak<Process>,
    // this has to be atomic usize because it is written to in assembly
    pub rsp: AtomicUsize,
    kernel_stack: KernelStack,
}

impl Thread {
    /// Creates a new thread, and returns the thread and its thread handle
    /// 
    /// If `kernel_stack` is owned, it must use the same allocator as the process (the drop implementation assumes this to be true)
    pub fn new(
        tid: Tid,
        name: String,
        process: Weak<Process>,
        kernel_stack: KernelStack,
        rsp: usize
    ) -> KResult<Arc<Thread>> {
        let allocer = process.alloc_ref();

        Arc::new(Thread {
            tid,
            name,
            status: AtomicUsize::new(ThreadState::Suspended.to_status(0)),
            process,
            rsp: AtomicUsize::new(rsp),
            kernel_stack,
        }, allocer)
    }

    /// This is the rsp value loaded when a syscall occurs for this thread
    pub fn syscall_rsp(&self) -> usize {
        self.kernel_stack.stack_top().as_usize()
    }

    /// Sets this threads state and incraments the generation
    pub fn set_state(&self, state: ThreadState) {
        self.status.fetch_update(
            Ordering::AcqRel,
            Ordering::Acquire,
            |old_status| {
                Some(state.to_status(old_status) + GENERATION_STEP_SIZE)
            },
        ).unwrap();
    }
}

#[derive(Debug, Clone)]
pub struct ThreadRef {
    thread: Weak<Thread>,
    generation: usize,
}

impl ThreadRef {
    pub fn get_thread(&self) -> Option<Arc<Thread>> {
        let thread = self.thread.upgrade()?;

        match thread.status.compare_exchange(
            self.generation,
            self.generation + GENERATION_STEP_SIZE,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => Some(thread),
            Err(_) => None,
        }
    }
}