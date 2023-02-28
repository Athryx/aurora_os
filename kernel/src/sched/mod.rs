pub mod kernel_stack;
mod thread;
mod thread_map;

use core::sync::atomic::Ordering;

pub use thread::{ThreadState, ThreadHandle, Thread};
use thread_map::ThreadMap;
use crate::arch::x64::IntDisable;
use crate::config::SCHED_TIME;
use crate::mem::MemOwner;
use crate::prelude::*;
use crate::arch::x64::asm_switch_thread;
use crate::sync::{IMutex, IMutexGuard};
use crate::container::Arc;
use crate::process::Process;

static THREAD_MAP: ThreadMap = ThreadMap::new();

/// This stores a reference to the current thread and process for easy retrieval
/// 
/// It is stored in the cpu local global variables
#[derive(Debug)]
pub struct SchedState {
    pub current_thread: Arc<Thread>,
    pub current_process: Arc<Process>,
}

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

/// All data used by the post switch handler
#[derive(Debug)]
pub struct PostSwitchData {
    old_thread_handle: MemOwner<ThreadHandle>,
    old_process: Arc<Process>,
    post_switch_action: PostSwitchAction,
}

/// Represents various different operations that need to be run after the thread is switched
/// 
/// They are run on the new thread just after loading that thread's stack,
/// but before reverting rflags and loading saved registers
/// 
/// This means interrupts are still disabled at this point and it is ok to hold resources
#[derive(Debug)]
pub enum PostSwitchAction {
    None,
}

/// This is the function that runs after thread switch
#[no_mangle]
extern "C" fn post_switch_handler(old_rsp: usize) {
    let mut post_switch_data = cpu_local_data().post_switch_data.lock();

    if let Some(post_switch_data) = post_switch_data.as_mut() {
        post_switch_data.old_process.num_threads_running.fetch_sub(1, Ordering::AcqRel);
        post_switch_data.old_thread_handle.thread.rsp.store(old_rsp, Ordering::Release);

        match post_switch_data.old_thread_handle.state {
            _ => (),
        }

        match post_switch_data.post_switch_action {
            PostSwitchAction::None => (),
        }
    } else {
        panic!("post switch data was none after switching threads")
    }

    *post_switch_data = None;
}

/// Switches the current thread to the given state
/// 
/// Takes an int_disable to ensure interrupts are disabled,
/// and reverts interrupts to the prevoius mode just before switching threads
pub fn switch_current_thread_to(state: ThreadState, int_disable: IntDisable, post_switch_hook: PostSwitchAction) {
    let (mut new_thread_handle, new_thread, new_process) = loop {
        let next_thread_handle = THREAD_MAP.get_ready_thread();
    
        let next_thread = unsafe {
            next_thread_handle
            .ptr()
            .as_ref()
            .unwrap()
            .thread
            .clone()
        };

        let next_process = match next_thread.process.upgrade() {
            Some(process) => process,
            None => {
                // FIXME: figure out which allocator to use to drop thread handle
                todo!();
                continue;
            },
        };

        // we have to incrament num thread running before checking thread is alive
        // otherwise we might read is alive as false, it is immediately set to true,
        // the thread terminating the process could then read num_threads_running before we incrament it,
        // and conclude that no more threads are running and it is ready to clean up the process
        // even though we are about to switch to the new process
        next_process.num_threads_running.fetch_add(1, Ordering::AcqRel);
        if !next_process.is_alive.load(Ordering::Acquire) {
            next_process.num_threads_running.fetch_sub(1, Ordering::AcqRel);
            // FIXME: figure out which allocator to use to drop thread handle
            todo!();
            continue;
        }

        break (next_thread_handle, next_thread, next_process);
    };

    // swap out current thread handle
    let old_thread_handle = cpu_local_data()
        .current_thread_handle.swap(new_thread_handle.ptr_mut(), Ordering::AcqRel);
    let mut old_thread_handle = unsafe { MemOwner::from_raw(old_thread_handle) };

    // change all thread states that need to be changed
    old_thread_handle.state = state;
    new_thread_handle.state = ThreadState::Running;

    let mut global_sched_state = cpu_local_data().sched_state();

    // save old process to decrament running thread count in post switch handler
    let old_process = global_sched_state.current_process.clone();

    // change current thread and process
    global_sched_state.current_thread = new_thread.clone();
    global_sched_state.current_process = new_process.clone();

    drop(global_sched_state);

    // set post switch data
    *cpu_local_data().post_switch_data.lock() = Some(PostSwitchData {
        old_thread_handle,
        old_process,
        post_switch_action: post_switch_hook,
    });

    cpu_local_data().last_thread_switch_nsec.store(cpu_local_data().local_apic().nsec(), Ordering::Release);

    let new_rsp = new_thread.rsp.load(Ordering::Acquire);
    let new_addr_space = new_process.get_cr3();
    unsafe {
        asm_switch_thread(int_disable.old_is_enabled(), new_rsp, new_addr_space);
    }
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