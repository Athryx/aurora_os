use core::sync::atomic::Ordering;

use spin::Once;

pub use thread::{ThreadState, Thread, ThreadRef, Tid, WakeReason};
use thread_map::ThreadMap;
use crate::alloc::root_alloc_ref;
use crate::arch::x64::IntDisable;
use crate::config::SCHED_TIME;
use crate::prelude::*;
use crate::sync::IMutex;
use crate::arch::x64::asm_switch_thread;
use crate::container::Arc;
use crate::process::{Process, get_kernel_process};
use timeout_queue::TimeoutQueue;

pub mod kernel_stack;
mod thread;
mod thread_map;
mod timeout_queue;

static THREAD_MAP: Once<ThreadMap> = Once::new();
static TIMEOUT_QUEUE: Once<IMutex<TimeoutQueue>> = Once::new();

pub fn thread_map() -> &'static ThreadMap {
    THREAD_MAP.get().unwrap()
}

pub fn timeout_queue() -> &'static IMutex<TimeoutQueue> {
    TIMEOUT_QUEUE.get().unwrap()
}

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

    timeout_queue().lock().wake_threads(current_nsec);

    if current_nsec - last_switch_nsec > SCHED_TIME.as_nanos() as u64 {
        let _ = switch_current_thread_to(
            ThreadState::Ready,
            IntDisable::new(),
            PostSwitchAction::InsertReadyQueue,
            true,
        );
    }
}

/// Called when an ipi_exit ipi occurs, and potentialy exits the current thread
pub fn exit_handler() {
    if cpu_local_data().current_process().is_alive.load(Ordering::Acquire) {
        switch_current_thread_to(
            ThreadState::Dead,
            IntDisable::new(),
            PostSwitchAction::None,
            true,
        ).expect("thread terminated and there were no more threads to run");
    }
}

/// All data used by the post switch handler
#[derive(Debug)]
pub struct PostSwitchData {
    old_thread: Arc<Thread>,
    old_process: Arc<Process>,
    post_switch_action: PostSwitchAction,
    send_eoi: bool,
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
    /// Inserts the thread into the ready queue to be run again
    InsertReadyQueue,
    /// Inserts the thread into the timeout queue to wake up at the given nanosecond
    SetTimeout(u64),
}

/// This is the function that runs after thread switch
#[no_mangle]
extern "C" fn post_switch_handler(old_rsp: usize) {
    let mut post_switch_data = cpu_local_data().post_switch_data.lock();
    let PostSwitchData {
        old_thread,
        post_switch_action,
        send_eoi,
        ..
    } = (*post_switch_data).take()
        .expect("post switch data was none after switching threads");

    old_thread.rsp.store(old_rsp, Ordering::Release);

    match post_switch_action {
        PostSwitchAction::None => (),
       // FIXME: don't panic on out of memory here
        PostSwitchAction::InsertReadyQueue => thread_map()
            .insert_ready_thread(Arc::downgrade(&old_thread))
            .expect("failed to add thread to ready queue"),
        // FIXME: don't panic on out of memory here
        PostSwitchAction::SetTimeout(timeout_nsec) => timeout_queue()
            .lock()
            .insert_thread(ThreadRef::new(&old_thread), timeout_nsec)
            .expect("failed to add thread to timeout queue")
    }

    if send_eoi {
        cpu_local_data().local_apic().eoi();
    }
}

/// Represents an error that occurs when calling [`switch_current_thread_to`]
#[derive(Debug)]
pub enum ThreadSwitchToError {
    /// There are no availabel threads to switch to
    NoAvailableThreads,
}

/// Switches the current thread to the given state
/// 
/// Takes an int_disable to ensure interrupts are disabled,
/// and reverts interrupts to the prevoius mode just before switching threads
/// 
/// Returns None if there were no available threads to switch to
pub fn switch_current_thread_to(state: ThreadState, _int_disable: IntDisable, post_switch_hook: PostSwitchAction, send_eoi: bool) -> Result<(), ThreadSwitchToError> {
    assert!(!matches!(state, ThreadState::Running), "cannot switch current thread to running state");

    let (new_thread, new_process) = thread_map().get_next_thread_and_process()
        .ok_or(ThreadSwitchToError::NoAvailableThreads)?;

    let mut global_sched_state = cpu_local_data().sched_state();

    // save old thread and old process to decrament running thread count in post switch handler
    let old_thread = global_sched_state.current_thread.clone();
    let old_process = global_sched_state.current_process.clone();

    // change all thread states that need to be changed
    old_thread.set_state(state);
    new_thread.set_state(ThreadState::Running);

    // update thread running count
    old_process.num_threads_running.fetch_sub(1, Ordering::AcqRel);
    new_process.num_threads_running.fetch_add(1, Ordering::AcqRel);

    // get the new rsp and address space we have to switch to
    let new_rsp = new_thread.rsp.load(Ordering::Acquire);
    let new_addr_space = new_process.get_cr3();

    // set syscall rsp
    cpu_local_data().syscall_rsp.store(new_thread.syscall_rsp(), Ordering::Release);
    // set interrupt rsp (rsp0 in tss is used when cpl of interrupts changes)
    cpu_local_data().tss.lock().rsp0 = new_thread.syscall_rsp() as u64;

    // change current thread and process in the scheduler state
    cpu_local_data().current_process_addr.store(Arc::as_ptr(&new_process) as usize, Ordering::Release);
    global_sched_state.current_thread = new_thread;
    global_sched_state.current_process = new_process;

    drop(global_sched_state);

    // set post switch data
    *cpu_local_data().post_switch_data.lock() = Some(PostSwitchData {
        old_thread,
        old_process,
        post_switch_action: post_switch_hook,
        send_eoi,
    });

    // update last switch time
    cpu_local_data().last_thread_switch_nsec.store(cpu_local_data().local_apic().nsec(), Ordering::Release);

    // at this point we are holding no resources that need to be dropped except for the int_disable, so it is good to switch
    unsafe {
        asm_switch_thread(new_rsp, new_addr_space);
    }

    Ok(())
}

pub fn init() {
    THREAD_MAP.call_once(|| ThreadMap::new(root_alloc_ref()));
    TIMEOUT_QUEUE.call_once(|| IMutex::new(TimeoutQueue::new(root_alloc_ref())));
}

/// Creates an idle thread and sets up scheduler from the currently executing thread and its stack
pub fn init_cpu_local(stack: AVirtRange) -> KResult<()> {
    let kernel_process = get_kernel_process();

    let thread = kernel_process.create_idle_thread(
        String::from_str(root_alloc_ref(), "idle_thread")?,
        stack,
    )?;

    cpu_local_data().syscall_rsp.store(thread.syscall_rsp(), Ordering::Release);
    cpu_local_data().current_process_addr.store(Arc::as_ptr(&kernel_process) as usize, Ordering::Release);

    cpu_local_data().set_sched_state(SchedState {
        current_thread: thread,
        current_process: kernel_process,
    });

    Ok(())
}