pub mod kernel_stack;
mod thread;
mod thread_map;

use core::sync::atomic::Ordering;

pub use thread::{ThreadState, ThreadHandle, Thread, Tid};
use thread_map::ThreadMap;
use crate::alloc::root_alloc_ref;
use crate::arch::x64::IntDisable;
use crate::config::SCHED_TIME;
use crate::mem::MemOwner;
use crate::prelude::*;
use crate::arch::x64::asm_switch_thread;
use crate::container::Arc;
use crate::process::{Process, get_kernel_process};

pub static THREAD_MAP: ThreadMap = ThreadMap::new();

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
        let _ = switch_current_thread_to(
            ThreadState::Ready,
            IntDisable::new(),
            PostSwitchAction::SendEoi,
        );
    }
}

/// Called when an ipi_exit ipi occurs, and potentialy exits the current thread
pub fn exit_handler() {
    switch_current_thread_to(
        ThreadState::Dead { try_destroy_process: true },
        IntDisable::new(),
        PostSwitchAction::SendEoi,
    ).expect("thread terminated and there were no more threads to run");
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
    /// Does nothing special after switching threads
    None,
    /// Sends an eoi after switching threads
    SendEoi,
}

/// This is the function that runs after thread switch
#[no_mangle]
extern "C" fn post_switch_handler(old_rsp: usize) {
    let mut post_switch_data = cpu_local_data().post_switch_data.lock();
    let PostSwitchData {
        old_thread_handle,
        old_process,
        post_switch_action,
    } = core::mem::replace(&mut *post_switch_data, None)
        .expect("post switch data was none after switching threads");

    let num_threads_running = old_process.num_threads_running.fetch_sub(1, Ordering::AcqRel) - 1;
    old_thread_handle.thread.rsp.store(old_rsp, Ordering::Release);

    match old_thread_handle.state {
        ThreadState::Running => unreachable!(),
        ThreadState::Ready => THREAD_MAP.insert_ready_thread(old_thread_handle),
        ThreadState::Dead { try_destroy_process } => {
            if try_destroy_process && num_threads_running == 0 {
                // Safety: at this point no more threads are running on the process
                // and no more will try in the future
                unsafe {
                    old_process.release_strong_capability();
                }
            }
            unsafe {
                ThreadHandle::dealloc(old_thread_handle);
            }
        },
        ThreadState::Suspend { .. } => THREAD_MAP.insert_suspended_thread(old_thread_handle),
        ThreadState::SuspendTimeout { .. } => THREAD_MAP.insert_suspended_timeout_thread(old_thread_handle),
    }

    match post_switch_action {
        PostSwitchAction::None => (),
        PostSwitchAction::SendEoi => cpu_local_data().local_apic().eoi(),
    }
}

/// Represents an error that occurs when calling [`switch_current_thread_to`]
#[derive(Debug)]
pub enum ThreadSwitchToError {
    /// There are no availabel threads to switch to
    NoAvailableThreads,
    /// An invalid state to switch to was passed in (ThreadState::Running)
    InvalidState,
}

/// Switches the current thread to the given state
/// 
/// Takes an int_disable to ensure interrupts are disabled,
/// and reverts interrupts to the prevoius mode just before switching threads
/// 
/// Returns None if there were no available threads to switch to
pub fn switch_current_thread_to(state: ThreadState, _int_disable: IntDisable, post_switch_hook: PostSwitchAction) -> Result<(), ThreadSwitchToError> {
    if matches!(state, ThreadState::Running) {
        return Err(ThreadSwitchToError::InvalidState);
    }

    let (mut new_thread_handle, new_thread, new_process) = loop {
        let next_thread_handle = THREAD_MAP.get_ready_thread()
            .ok_or(ThreadSwitchToError::NoAvailableThreads)?;
    
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
                // Safety: we removed the thread handle from the thread map so this is the only reference
                unsafe {
                    ThreadHandle::dealloc(next_thread_handle);
                }

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
            
            // Safety: we removed the thread handle from the thread map so this is the only reference
            unsafe {
                ThreadHandle::dealloc(next_thread_handle);
            }

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

    // get the new rsp and address space we have to switch to
    let new_rsp = new_thread.rsp.load(Ordering::Acquire);
    let new_addr_space = new_process.get_cr3();

    // set syscall rsp
    cpu_local_data().syscall_rsp.store(new_thread.syscall_rsp(), Ordering::Release);

    // change current thread and process
    global_sched_state.current_thread = new_thread;
    global_sched_state.current_process = new_process;

    drop(global_sched_state);

    // set post switch data
    *cpu_local_data().post_switch_data.lock() = Some(PostSwitchData {
        old_thread_handle,
        old_process,
        post_switch_action: post_switch_hook,
    });

    cpu_local_data().last_thread_switch_nsec.store(cpu_local_data().local_apic().nsec(), Ordering::Release);

    // at this point we are holding no resources that need to be dropped except for the int_disable, os it is good to switch
    unsafe {
        asm_switch_thread(new_rsp, new_addr_space);
    }

    Ok(())
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
    todo!()
}

/// Creates an idle thread and sets up scheduler from the currently executing thread and its stack
pub fn init(stack: AVirtRange) -> KResult<()> {
    let kernel_process = get_kernel_process();

    let (thread, thread_handle) = kernel_process.create_idle_thread(
        String::from_str(root_alloc_ref().downgrade(), "idle_thread")?,
        stack,
    )?;

    cpu_local_data().set_sched_state(SchedState {
        current_thread: thread,
        current_process: kernel_process,
    });

    // TODO: maybe put idle thread in cpu local global variable so idle thread is per cpu
    // reducing lock pressure on thread list
    cpu_local_data().current_thread_handle.store(thread_handle.ptr_mut(), Ordering::Release);

    Ok(())
}