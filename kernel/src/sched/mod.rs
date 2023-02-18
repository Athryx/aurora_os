pub mod kernel_stack;
mod thread;
mod thread_map;

use core::sync::atomic::Ordering;

pub use thread::{ThreadState, ThreadHandle, Thread};
use thread_map::ThreadMap;
use crate::arch::x64::IntDisable;
use crate::config::SCHED_TIME;
use crate::prelude::*;
use crate::sync::{IMutex, IMutexGuard};
use crate::container::Arc;
use crate::process::Process;

static THREAD_MAP: ThreadMap = ThreadMap::new();

/// Called every time the local apic timer ticks
pub fn timer_handler() {
    let current_nsec = cpu_local_data().local_apic().nsec();
    let last_switch_nsec = cpu_local_data().last_thread_switch_nsec.load(Ordering::Acquire);

    if current_nsec - last_switch_nsec > SCHED_TIME.as_nanos() as u64 {
        // FIXME: send eoi
        switch_current_thread_to(
            ThreadState::Ready,
            IntDisable::new(),
            PostSwitchAction::None
        );
    }
}

/// Called when an ipi_exit ipi occurs, and potentialy exits the current thread
pub fn exit_handler() {
    // FIXME: send eoi
    switch_current_thread_to(
        ThreadState::Dead { try_destroy_process: true },
        IntDisable::new(),
        PostSwitchAction::None
    );
}

/// Represents various different operations that need to be run after the thread is switched
/// 
/// They are run on the new thread just after loading that thread's stack,
/// but before reverting rflags and loading saved registers
/// 
/// This means interrupts are still disabled at this point and it is ok to hold resources
#[derive(Debug)]
pub enum PostSwitchAction {
    DestroyThread(*const ThreadHandle),
    None,
}

/// This is the function that runs post thread switch
#[no_mangle]
extern "C" fn post_switch_handler() {
    unimplemented!()
}

/// Switches the current thread to the given state
/// 
/// Takes an int_disable to ensure interrupts are disabled,
/// and reverts interrupts to the prevoius mode just before switching threads
pub fn switch_current_thread_to(state: ThreadState, int_disable: IntDisable, post_switch_hook: PostSwitchAction) {
    unimplemented!()
}

/// Represents an error that prevented another thread from being switcued
/// 
/// This is only returned by [`switch_other_thread_to`], [`switch_current_thread_to`] always succeeds
pub enum ThreadSwitchError {
    /// The specified thread was currently running
    ThreadRunning,
    /// The specified thrad was currently dead or was part of a dead process
    ThreadDied,
}

pub fn switch_other_thread_to(thread_handle: *const ThreadHandle, state: ThreadState) -> Result<(), ThreadSwitchError> {
    unimplemented!()
}

pub fn init() -> KResult<()> {
    Ok(())
}

pub fn ap_init(stack_addr: usize) -> KResult<()> {
    Ok(())
}