use sys::{CapId, ThreadDestroyFlags, Tid};

use crate::{prelude::*,
    cap::{CapFlags, Capability},
    arch::x64::IntDisable,
    alloc::{PaRef, HeapRef},
    process::ThreadStartMode,
    sched::{switch_current_thread_to, ThreadState, PostSwitchAction},
};
use crate::process::Process;
use super::options_weak_autodestroy;

/// creates a new process with name `name`
/// 
/// in order to avoid memory leaks due to reference cycles, process_new always returns an unupgradable weak capability
/// the kernel keeps 1 internal strong refernce to each process when it is created
/// in order to destroy the process, call process_exit to destroy the strong refernce to the process, which will dealloc the process
/// the process is not freed when all weak references are destroyed
///
/// # Options
/// bits 0-2 (process_cap_flags): CapPriv representing read, prod, and write privalidges of new capability
///
/// # Required Capability Permissions:
/// `allocator`: cap_prod
/// `spawner`: cap_prod
///
/// # Returns
/// pocess: capability of new process
// TODO: process name
pub fn process_new(options: u32, allocator_id: usize, spawner_id: usize) -> KResult<usize> {
    let weak_auto_destroy = options_weak_autodestroy(options);
    let process_cap_flags = CapFlags::from_bits_truncate(get_bits(options as usize, 0..2));

    let _int_disable = IntDisable::new();

    let current_process = cpu_local_data().current_process();

    let allocator = current_process.cap_map()
        .get_allocator_with_perms(allocator_id, CapFlags::PROD, weak_auto_destroy)?
        .into_inner();

    let spawner = current_process.cap_map()
        .get_spawner_with_perms(spawner_id, CapFlags::PROD, weak_auto_destroy)?
        .into_inner();

    let page_allocator = PaRef::from_arc(allocator.clone());
    let heap_allocator = HeapRef::from_arc(allocator);

    // TODO: process name
    let name = String::new(heap_allocator.clone());
    let mut new_process = Process::new(
        page_allocator,
        heap_allocator,
        name,
    )?;

    new_process.id = CapId::null_flags(process_cap_flags, true);

    spawner.add_process(new_process.inner().clone())?;

    Ok(current_process.cap_map().insert_process(Capability::Weak(new_process))?.into())
}

/// destroys the kernel's strong refernce to the process, which will cause the process to exit
///
/// # Required Capability Permissions
/// `process`: cap_write
pub fn process_exit(options: u32, process_id: usize) -> KResult<()> {
    let weak_auto_destroy = options_weak_autodestroy(options);

    let _int_disable = IntDisable::new();

    let process = cpu_local_data()
        .current_process()
        .cap_map()
        .get_process_with_perms(process_id, CapFlags::WRITE, weak_auto_destroy)?
        .into_inner();

    // no other object references are held, safe to call
    Process::exit(process);

    Ok(())
}

/// creates a new thread with name `name` in `process` and returns its id
/// 
/// the new thread will have its rip and rsp registers set according to the values passed in
/// 4 additional registers can be passed in, and they correspond to certain registers that will be set in the new thread
/// 
/// on x86_64, the registers correspond as follows:
/// `r1`: rbx
/// `r2`: rdx
/// `r3`: rsi
/// `r4`: rdi
///
/// all other registers are set to 0
///
/// # Options
/// bit 0 (thread_autostart): if set, the thread will start as soon as it is created
/// otherwise, it will start in a suspended state
///
/// # Required Capability Permissions
/// `process`: cap_write
///
/// # Returns
/// tid: thread id of new thread
// TODO: thread name
pub fn thread_new(
    options: u32,
    process_id: usize,
    rip: usize,
    rsp: usize,
) -> KResult<usize> {
    let weak_auto_destroy = options_weak_autodestroy(options);

    let _int_disable = IntDisable::new();

    let process = cpu_local_data()
        .current_process()
        .cap_map()
        .get_process_with_perms(process_id, CapFlags::WRITE, weak_auto_destroy)?
        .into_inner();

    let thread_start_mode = if get_bits(options as usize, 0..1) == 1 {
        ThreadStartMode::Ready
    } else {
        ThreadStartMode::Suspended
    };

    // TODO: thread name
    let thread_name = String::new(process.heap_allocator());
    Ok(process.create_thread(
        thread_name,
        thread_start_mode,
        rip,
        rsp,
    )?.into())
}

/// yields the currently running thread and allows another ready thread to run
pub fn thread_yield() -> KResult<()> {
    let int_disable = IntDisable::new();

    // TODO: detect if the only idle thread running is idle thread, and don't yield if that is the case
    // panic safety: this should never fail because the idle thread should always ba available
    switch_current_thread_to(
        ThreadState::Ready,
        int_disable,
        PostSwitchAction::None,
        false
    ).expect("could not find thread to yield to");

    Ok(())
}

/// destroys the specified thread or destroys the currently running thread
/// if thread_destroy_other is set, the specified thread must be suspended to ba able to be destroyed
/// 
/// # Options
/// bit 0 (thread_destroy_other): will destroy a thread with {thread_id} in {process}
/// if not set, will destroy the calling thread
/// 
/// # Required Capability Permissions
/// `process`: cap_write
/// 
/// # Syserr code
/// InvlOp: thread_destroy_other was set and the other thread was not suspended
pub fn thread_destroy(options: u32, process_id: usize, thread_id: usize) -> KResult<()> {
    let weak_auto_destroy = options_weak_autodestroy(options);
    let flags = ThreadDestroyFlags::from_bits_truncate(options);
    let thread_id = Tid::from(thread_id);

    let int_disable = IntDisable::new();

    if flags.contains(ThreadDestroyFlags::DESTROY_OTHER) {
        let process = cpu_local_data()
            .current_process()
            .cap_map()
            .get_process_with_perms(process_id, CapFlags::WRITE, weak_auto_destroy)?
            .into_inner();

        process.destroy_suspended_thread(thread_id)
    } else {
        switch_current_thread_to(
            ThreadState::Dead,
            int_disable,
            PostSwitchAction::None,
            false,
        ).unwrap();

        Ok(())
    }
}

/// suspends the currently running thread and waits for the thread to be resumed by another thread
///
/// # Options
/// bit 0 (suspend_timeout): the thread will be woken `timeout_nsec` nanoseconds after boot if it has not already been woken up
pub fn thread_suspend(options: u32, timeout_nsec: usize) -> KResult<()> {
    let int_disable = IntDisable::new();

    if get_bits(options as usize, 0..1) == 1 {
        switch_current_thread_to(
            ThreadState::Suspended,
            int_disable,
            PostSwitchAction::SetTimeout(timeout_nsec as u64),
            false,
        ).expect("could not find idle thread to switch to");
    } else {
        switch_current_thread_to(
            ThreadState::Suspended,
            int_disable,
            PostSwitchAction::None,
            false,
        ).expect("could not find idle thread to switch to");
    }

    Ok(())
}

/// resumes a thread that was previously suspended
/// 
/// # Required Capability Permissions
/// `process`: cap_write
/// 
/// # Syserr Code
/// InvlOp: the specified thread is not currently suspended
pub fn thread_resume(options: u32, process_id: usize, thread_id: usize) -> KResult<()> {
    let weak_auto_destroy = options_weak_autodestroy(options);
    let thread_id = Tid::from(thread_id);

    let _int_disable = IntDisable::new();

    let process = cpu_local_data()
        .current_process()
        .cap_map()
        .get_process_with_perms(process_id, CapFlags::WRITE, weak_auto_destroy)?
        .into_inner();

    process.resume_suspended_thread(thread_id)
}