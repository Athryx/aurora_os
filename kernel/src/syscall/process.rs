use crate::{prelude::*,
    cap::{CapFlags, Capability},
    arch::x64::IntDisable,
    alloc::{PaRef, OrigRef},
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

    let _int_disable = IntDisable::new();

    let current_process = cpu_local_data().current_process();

    let allocator = current_process.cap_map()
        .get_allocator_with_perms(allocator_id, CapFlags::PROD, weak_auto_destroy)?;

    let spawner = current_process.cap_map()
        .get_spawner_with_perms(spawner_id, CapFlags::PROD, weak_auto_destroy)?;

    let page_allocator = PaRef::from_arc(allocator.clone());
    let heap_allocator = OrigRef::from_arc(allocator);

    // TODO: process name
    let name = String::new(heap_allocator.downgrade());
    let new_process = Process::new(
        page_allocator,
        heap_allocator,
        name,
    )?;

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
        .get_process_with_perms(process_id, CapFlags::WRITE, weak_auto_destroy)?;

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
    r1: usize,
    r2: usize,
    r3: usize,
    r4: usize,
) -> KResult<usize> {
    let weak_auto_destroy = options_weak_autodestroy(options);

    let _int_disable = IntDisable::new();

    let process = cpu_local_data()
        .current_process()
        .cap_map()
        .get_process_with_perms(process_id, CapFlags::WRITE, weak_auto_destroy)?;

    let thread_start_mode = if get_bits(options as usize, 0..1) == 1 {
        ThreadStartMode::Ready
    } else {
        ThreadStartMode::Suspended
    };

    // TODO: thread name
    let thread_name = String::new(process.heap_allocator().downgrade());
    Ok(process.create_thread(
        thread_name,
        thread_start_mode,
        rip,
        rsp,
        (r1, r2, r3, r4),
    )?.into())
}

/// yields the currently running thread and allows another ready thread to run
pub fn thread_yield() -> KResult<()> {
    let int_disable = IntDisable::new();

    // TODO: detect if the only idle thread running is idle thread, and don't yield if that is the case
    // panic safety: this should never fail because the idle thread should always ba available
    switch_current_thread_to(ThreadState::Ready, int_disable, PostSwitchAction::None)
        .expect("could not find thread to yield to");

    Ok(())
}

/// suspends the currently running thread and waits for the thread to be resumed by another thread
///
/// # Options
/// bit 0 (suspend_timeout): the thread will be woken `timeout_nsec` nanoseconds after boot if it has not already been woken up
pub fn thread_suspend(options: u32, timeout_nsec: usize) -> KResult<()> {
    let int_disable = IntDisable::new();

    if get_bits(options as usize, 0..1) == 1 {
        switch_current_thread_to(
            ThreadState::SuspendTimeout {
                for_event: false,
                until_nanosecond: timeout_nsec as u64,
            },
            int_disable,
            PostSwitchAction::None,
        ).expect("could not find idle thread to switch to");
    } else {
        switch_current_thread_to(
            ThreadState::Suspend { for_event: false },
            int_disable,
            PostSwitchAction::None,
        ).expect("could not find idle thread to switch to");
    }

    Ok(())
}