use sys::{CapFlags, ThreadNewFlags, ThreadSuspendFlags, ThreadDestroyFlags, ThreadProperty};

use crate::alloc::HeapRef;
use crate::arch::x64::IntDisable;
use crate::cap::{WeakCapability, Capability};
use crate::container::Arc;
use crate::cap::capability_space::CapabilitySpace;
use crate::prelude::*;
use crate::sched::{ThreadGroup, ThreadStartMode, switch_current_thread_to, ThreadState, PostSwitchAction, WakeReason, Thread};
use super::options_weak_autodestroy;

pub fn thread_new(
    options: u32,
    allocator_id: usize,
    thread_group_id: usize,
    addr_space_id: usize,
    cap_space_id: usize,
    rip: usize,
    rsp: usize,
) -> KResult<(usize, usize)> {
    let weak_auto_destroy = options_weak_autodestroy(options);
    let flags = ThreadNewFlags::from_bits_truncate(options);

    let start_mode = if flags.contains(ThreadNewFlags::THREAD_AUTOSTART) {
        ThreadStartMode::Ready
    } else {
        ThreadStartMode::Suspended
    };

    let _int_disable = IntDisable::new();

    let cspace = CapabilitySpace::current();

    let allocator = cspace
        .get_allocator_with_perms(allocator_id, CapFlags::PROD, weak_auto_destroy)?
        .into_inner();
    let heap_ref = HeapRef::from_arc(allocator);

    let thread_group = cspace
        .get_thread_group_with_perms(thread_group_id, CapFlags::PROD, weak_auto_destroy)?
        .into_inner();

    let addr_space = cspace
        .get_address_space_with_perms(addr_space_id, CapFlags::PROD, weak_auto_destroy)?
        .into_inner();

    let (new_cspace, new_cspace_cap_id) = if flags.contains(ThreadNewFlags::CREATE_CAPABILITY_SPACE) {
        let new_cspace = Arc::new(
            CapabilitySpace::new(heap_ref.clone()),
            heap_ref.clone(),
        )?;

        let new_cspace_cap = WeakCapability::new_flags(
            Arc::downgrade(&new_cspace),
            CapFlags::READ | CapFlags::PROD | CapFlags::WRITE,
        );

        let new_cspace_cap_id = cspace
            .insert_capability_space(Capability::Weak(new_cspace_cap))?;

        (new_cspace, Some(new_cspace_cap_id))
    } else {
        (cspace
            .get_capability_space_with_perms(cap_space_id, CapFlags::WRITE, weak_auto_destroy)?
            .into_inner(), None)
    };

    let name = String::new(heap_ref);

    let new_thread_result = ThreadGroup::create_thread(
        &thread_group,
        addr_space,
        new_cspace,
        name,
        start_mode,
        rip,
        rsp,
    );

    let new_thread = match new_thread_result {
        Ok(new_thread) => new_thread,
        Err(error) => {
            // remove the new capability space we just created if creating the thread fails
            if let Some(cspace_id) = new_cspace_cap_id {
                // ignore error, someone else could have removed cspace
                let _ = cspace.remove_capability_space(cspace_id);
            }

            return Err(error);
        }
    };

    let new_thread_capability = WeakCapability::new_flags(
        Arc::downgrade(&new_thread),
        CapFlags::READ | CapFlags::PROD | CapFlags::WRITE,
    );

    let insert_thread_result = cspace.insert_thread(Capability::Weak(new_thread_capability));

    let new_thread_cap_id = match insert_thread_result {
        Ok(cap_id) => cap_id,
        Err(error) => {
            // remove the new capability space we just created if inserting the thread fails
            if let Some(cspace_id) = new_cspace_cap_id {
                // ignore error, someone else could have removed cspace
                let _ = cspace.remove_capability_space(cspace_id);
            }

            // remove thread from thread group
            thread_group.remove_thread(&new_thread);

            return Err(error);
        }
    };

    let new_cspace_cap_id = new_cspace_cap_id
        .map(|cap_id| cap_id.into())
        .unwrap_or(cap_space_id);

    Ok((new_thread_cap_id.into(), new_cspace_cap_id))
}

/// yields the currently running thread and allows another ready thread to run
pub fn thread_yield() -> KResult<()> {
    let int_disable = IntDisable::new();

    // TODO: detect if the only idle thread running is idle thread, and don't yield if that is the case
    // panic safety: this should never fail because the idle thread should always ba available
    switch_current_thread_to(
        ThreadState::Ready,
        int_disable,
        PostSwitchAction::InsertReadyQueue,
        false
    ).expect("could not find thread to yield to");

    Ok(())
}

pub fn thread_destroy(options: u32, thread_id: usize) -> KResult<()> {
    let weak_auto_destroy = options_weak_autodestroy(options);
    let flags = ThreadDestroyFlags::from_bits_truncate(options);

    let int_disable = IntDisable::new();

    if flags.contains(ThreadDestroyFlags::DESTROY_OTHER) {
        let thread = CapabilitySpace::current()
            .get_thread_with_perms(thread_id, CapFlags::WRITE, weak_auto_destroy)?
            .into_inner();

        Thread::destroy_suspended_thread(&thread)
    } else {
        switch_current_thread_to(
            ThreadState::Dead,
            int_disable,
            PostSwitchAction::None,
            false,
        ).unwrap();

        // execution should never reach here
        Err(SysErr::Unknown)
    }
}

/// suspends the currently running thread and waits for the thread to be resumed by another thread
///
/// # Options
/// bit 0 (suspend_timeout): the thread will be woken `timeout_nsec` nanoseconds after boot if it has not already been woken up
pub fn thread_suspend(options: u32, timeout_nsec: usize) -> KResult<()> {
    let flags = ThreadSuspendFlags::from_bits_truncate(options);

    let int_disable = IntDisable::new();

    if flags.contains(ThreadSuspendFlags::SUSPEND_TIMEOUT) {
        switch_current_thread_to(
            ThreadState::Suspended,
            int_disable,
            PostSwitchAction::SetTimeout(timeout_nsec as u64),
            false,
        ).expect("could not find idle thread to switch to");

        if cpu_local_data().current_thread().wake_reason() == WakeReason::Timeout {
            Err(SysErr::OkTimeout)
        } else {
            Ok(())
        }
    } else {
        switch_current_thread_to(
            ThreadState::Suspended,
            int_disable,
            PostSwitchAction::None,
            false,
        ).expect("could not find idle thread to switch to");

        Ok(())
    }
}

pub fn thread_resume(options: u32, thread_id: usize) -> KResult<()> {
    let weak_auto_destroy = options_weak_autodestroy(options);

    let _int_disable = IntDisable::new();

    let thread = CapabilitySpace::current()
        .get_thread_with_perms(thread_id, CapFlags::WRITE, weak_auto_destroy)?
        .into_inner();

    Thread::resume_suspended_thread(&thread)
}

pub fn thread_set_property(_options: u32, property: usize, data: usize) -> KResult<()> {
    let property = ThreadProperty::from_repr(property)
        .ok_or(SysErr::InvlArgs)?;

    let _int_disable = IntDisable::new();

    let current_thread = cpu_local_data().current_thread();

    match property {
        ThreadProperty::ThreadLocalPointer => {
            current_thread.set_thread_local_pointer(data);
            current_thread.load_thread_local_pointer();
        },
    }

    Ok(())
}