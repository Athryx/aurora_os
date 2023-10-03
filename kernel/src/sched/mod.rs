use core::sync::atomic::Ordering;

use spin::Once;

pub use thread::{ThreadState, Thread, ThreadRef, Tid, WakeReason};
pub use thread_group::{ThreadGroup, ThreadStartMode};
use thread_map::ThreadMap;
use crate::alloc::{root_alloc_ref, root_alloc_page_ref};
use crate::arch::x64::{IntDisable, set_cr3};
use crate::cap::address_space::AddressSpace;
use crate::cap::capability_space::CapabilitySpace;
use crate::config::SCHED_TIME;
use crate::prelude::*;
use crate::sync::IMutex;
use crate::arch::x64::asm_switch_thread;
use crate::container::Arc;
use timeout_queue::TimeoutQueue;
use kernel_stack::KernelStack;

pub mod kernel_stack;
mod thread;
mod thread_group;
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
    if !cpu_local_data().current_thread().is_alive() {
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

    let new_thread = thread_map().get_next_thread()
        .ok_or(ThreadSwitchToError::NoAvailableThreads)?;

    let mut global_sched_state = cpu_local_data().sched_state();

    let old_thread = global_sched_state.current_thread.clone();

    // change all thread states that need to be changed
    old_thread.set_state(state);
    new_thread.set_state(ThreadState::Running);

    // get the new rsp and address space we have to switch to
    let new_rsp = new_thread.rsp.load(Ordering::Acquire);
    let new_addr_space = new_thread.address_space().get_cr3().as_usize();

    new_thread.load_thread_local_pointer();

    // set syscall rsp
    cpu_local_data().syscall_rsp.store(new_thread.syscall_rsp(), Ordering::Release);
    // set interrupt rsp (rsp0 in tss is used when cpl of interrupts changes)
    cpu_local_data().tss.lock().rsp0 = new_thread.syscall_rsp() as u64;

    // change current thread and process in the scheduler state
    global_sched_state.current_thread = new_thread;

    drop(global_sched_state);

    // set post switch data
    *cpu_local_data().post_switch_data.lock() = Some(PostSwitchData {
        old_thread,
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

static KERNEL_THREAD_GROUP: Once<Arc<ThreadGroup>> = Once::new();
static KERNEL_ADDRESS_SPACE: Once<Arc<AddressSpace>> = Once::new();
static KERNEL_CAPABILITY_SPACE: Once<Arc<CapabilitySpace>> = Once::new();

/// Initializes thread group, address space, and capability space used by kernel threads
pub fn init_kernel_context() -> KResult<()> {
    let thread_group = Arc::new(
        ThreadGroup::new(root_alloc_page_ref(), root_alloc_ref()),
        root_alloc_ref(),
    )?;

    let address_space = Arc::new(
        AddressSpace::new(root_alloc_page_ref(), root_alloc_ref())?,
        root_alloc_ref(),
    )?;

    let capability_space = Arc::new(
        CapabilitySpace::new(root_alloc_ref()),
        root_alloc_ref(),
    )?;

    KERNEL_THREAD_GROUP.call_once(|| thread_group);
    KERNEL_ADDRESS_SPACE.call_once(|| address_space);
    KERNEL_CAPABILITY_SPACE.call_once(|| capability_space);

    Ok(())
}

/// Creates an idle thread and sets up scheduler from the currently executing thread and its stack
pub fn init_cpu_local(stack: AVirtRange) -> KResult<()> {
    let thread_group = KERNEL_THREAD_GROUP.get().unwrap();
    let address_space = KERNEL_ADDRESS_SPACE.get().unwrap();
    let capability_space = KERNEL_CAPABILITY_SPACE.get().unwrap();

    set_cr3(address_space.get_cr3().as_usize());

    let thread = Arc::new(
        Thread::new(
            String::from_str(root_alloc_ref(), "idle_thread")?,
            KernelStack::Existing(stack),
            // rsp will be set on next switch
            0,
            Arc::downgrade(thread_group),
            address_space.clone(),
            capability_space.clone(),
            root_alloc_ref(),
        ),
        root_alloc_ref(),
    )?;

    thread_group.add_thread(thread.clone())?;

    cpu_local_data().syscall_rsp.store(thread.syscall_rsp(), Ordering::Release);

    cpu_local_data().set_sched_state(SchedState {
        current_thread: thread,
    });

    Ok(())
}