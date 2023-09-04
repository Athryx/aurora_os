use core::sync::atomic::{AtomicUsize, Ordering, AtomicBool};

use crate::cap::CapObject;
use crate::cap::capability_space::CapabilitySpace;
use crate::cap::address_space::AddressSpace;
use crate::container::Arc;
use crate::sync::IMutex;
use super::kernel_stack::KernelStack;
use super::{thread_map, ThreadGroup};
use crate::container::Weak;
use crate::prelude::*;

use sys::CapType;
pub use sys::Tid;

/// Amount status must be incramented to change generation without changing ThreadState
const GENERATION_STEP_SIZE: usize = 0b100;

const THREAD_STATE_MASK: usize = 0b11;

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    pub fn to_status(self, old_status: usize) -> usize {
        (old_status & !THREAD_STATE_MASK) | self as usize
    }
}

/// Notifies a thread why it was woken up
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WakeReason {
    None,
    /// Thread was woken up do to a timeout finishing
    Timeout,
    /// Thread was woken up after sending or recieving a message from a channel
    MsgSendRecv {
        msg_size: usize,
    },
}

#[derive(Debug)]
pub struct Thread {
    name: String,
    status: AtomicUsize,
    wake_reason: IMutex<WakeReason>,
    // TODO: figure out if this is needed
    pub is_alive: AtomicBool,
    // this has to be atomic usize because it is written to in assembly
    pub rsp: AtomicUsize,
    kernel_stack: KernelStack,
    thread_group: Weak<ThreadGroup>,
    address_space: Arc<AddressSpace>,
    capability_space: Arc<CapabilitySpace>,
}

impl Thread {
    pub fn new(
        name: String,
        kernel_stack: KernelStack,
        rsp: usize,
        thread_group: Weak<ThreadGroup>,
        address_space: Arc<AddressSpace>,
        capability_space: Arc<CapabilitySpace>,
    ) -> Self {
        Thread {
            name,
            status: AtomicUsize::new(ThreadState::Suspended.to_status(0)),
            wake_reason: IMutex::new(WakeReason::None),
            is_alive: AtomicBool::new(true),
            rsp: AtomicUsize::new(rsp),
            kernel_stack,
            thread_group,
            address_space,
            capability_space,
        }
    }

    pub fn address_space(&self) -> &Arc<AddressSpace> {
        &self.address_space
    }

    pub fn capability_space(&self) -> &Arc<CapabilitySpace> {
        &self.capability_space
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

    /// Gets the wake reason of this thread
    pub fn wake_reason(&self) -> WakeReason {
        *self.wake_reason.lock()
    }

    /// Sets this threads state and incraments the generation, only if the old state is `old_state`
    /// 
    /// Returns true if the state was chenged
    pub fn transition_state(&self, old_state: ThreadState, new_state: ThreadState) -> bool {
        self.status.fetch_update(
            Ordering::AcqRel,
            Ordering::Acquire,
            |old_status| {
                if old_state != ThreadState::from_usize(old_status) {
                    None
                } else {
                    Some(new_state.to_status(old_status) + GENERATION_STEP_SIZE)
                }
            },
        ).is_ok()
    }

    pub fn is_current_thread(&self) -> bool {
        ptr::eq(
            self as *const Thread,
            Arc::as_ptr(&cpu_local_data().current_thread()),
        )
    }

    pub fn destroy_suspended_thread(thread: &Arc<Thread>) -> KResult<()> {
        if thread.transition_state(ThreadState::Suspended, ThreadState::Dead) {
            thread.is_alive.store(false, Ordering::Release);

            let Some(thread_group) = thread.thread_group.upgrade() else {
                return Ok(())
            };

            thread_group.remove_thread(thread);

            Ok(())
        } else {
            Err(SysErr::InvlOp)
        }
    }

    pub fn resume_suspended_thread(thread: &Arc<Thread>) -> KResult<()> {
        if thread.transition_state(ThreadState::Suspended, ThreadState::Ready) {
            // FIXME: don't panic on oom
            thread_map().insert_ready_thread(Arc::downgrade(thread))
                .expect("could not resume suepended thread");

            Ok(())
        } else {
            Err(SysErr::InvlOp)
        }
    }
}

impl CapObject for Thread {
    const TYPE: CapType = CapType::Thread;
}

#[derive(Debug, Clone)]
pub struct ThreadRef {
    thread: Weak<Thread>,
    generation: usize,
}

impl ThreadRef {
    /// This should only be used in the post switch handler for a thread that is suspended
    /// 
    /// # Panics
    /// 
    /// panics if the given thread is not suspended
    pub(super) fn new(thread: &Arc<Thread>) -> Self {
        let generation = thread.status.load(Ordering::Acquire);

        assert!(
            matches!(ThreadState::from_usize(generation), ThreadState::Suspended),
            "tried to make a thread ref for a thread which was not suspended",
        );

        ThreadRef {
            thread: Arc::downgrade(thread),
            generation,
        }
    }

    /// Returns a thread ref to the next generation of this thread
    /// 
    /// It assumes the thread's state will be suspended in the next generation
    pub fn future_ref(thread: &Arc<Thread>) -> Self {
        let generation = thread.status.load(Ordering::Acquire);
        let next_generation = ThreadState::Suspended.to_status(generation) + GENERATION_STEP_SIZE;

        ThreadRef {
            thread: Arc::downgrade(thread),
            generation: next_generation,
        }
    }

    /// Gets the thread and sets its status to Ready if it is alive adn the correct generation
    pub fn get_thread_as_ready(&self) -> Option<Arc<Thread>> {
        let thread = self.thread.upgrade()?;
        let ref_generation = self.generation & !THREAD_STATE_MASK;

        loop {
            match thread.status.compare_exchange(
                self.generation,
                ThreadState::Ready.to_status(self.generation) + GENERATION_STEP_SIZE,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return Some(thread),
                Err(old_status) => {
                    let old_generation = old_status & !THREAD_STATE_MASK;

                    // this will cause loop to spin if the ref generation is greater than the old generation,
                    // which gives time for a thread to adjust its state to be consistant with a `ThreadRef::future_ref`
                    if old_generation >= ref_generation {
                        return None;
                    }
                },
            }
        }
    }

    /// Attempts to move the thread to the ready list, returns true on success and false on failure
    pub fn move_to_ready_list(&self, wake_reason: WakeReason) -> bool {
        let Some(thread) = self.get_thread_as_ready() else {
            return false;
        };

        *thread.wake_reason.lock() = wake_reason;

        // FIXME: don't have oom here
        thread_map().insert_ready_thread(Arc::downgrade(&thread))
            .expect("failed to insert thread into ready list");

        true
    }
}