use sys::{CapFlags, EventPoolAwaitFlags};

use crate::alloc::{HeapRef, PaRef};
use crate::cap::{StrongCapability, Capability};
use crate::cap::capability_space::CapabilitySpace;
use crate::container::Arc;
use crate::event::{EventPool, AwaitStatus};
use crate::prelude::*;
use crate::arch::x64::IntDisable;
use crate::sched::{switch_current_thread_to, ThreadState, PostSwitchAction, WakeReason};

use super::options_weak_autodestroy;

pub fn event_pool_new(options: u32, allocator_id: usize, max_size: usize) -> KResult<usize> {
    let weak_auto_destroy = options_weak_autodestroy(options);
    let event_pool_size = Size::try_from_pages(max_size)
        .ok_or(SysErr::Overflow)?;

    let _int_disable = IntDisable::new();

    let cspace = CapabilitySpace::current();

    let allocator = cspace
        .get_allocator_with_perms(allocator_id, CapFlags::PROD, weak_auto_destroy)?
        .into_inner();
    let pa_ref = PaRef::from_arc(allocator.clone());
    let heap_ref = HeapRef::from_arc(allocator);

    let event_pool = StrongCapability::new_flags(
        Arc::new(
            EventPool::new(pa_ref, heap_ref.clone(), event_pool_size)?,
            heap_ref,
        )?,
        CapFlags::all(),
    );

    let cap_id = cspace.insert_event_pool(Capability::Strong(event_pool))?;

    Ok(cap_id.into())
}

pub fn event_pool_map(
    options: u32,
    addr_space_id: usize,
    event_pool_id: usize,
    addr: usize,
) -> KResult<usize> {
    let weak_auto_destroy = options_weak_autodestroy(options);
    let addr = VirtAddr::try_new_aligned(addr)?;

    let _int_disable = IntDisable::new();

    let cspace = CapabilitySpace::current();

    let addr_space = cspace
        .get_address_space_with_perms(addr_space_id, CapFlags::WRITE, weak_auto_destroy)?
        .into_inner();

    let event_pool = cspace
        .get_event_pool_with_perms(event_pool_id, CapFlags::WRITE, weak_auto_destroy)?
        .into_inner();

    EventPool::map_event_pool(event_pool, addr_space, addr)
        .map(Size::pages_rounded)
}

pub fn event_pool_await(options: u32, event_pool_id: usize, timeout: usize) -> KResult<(usize, usize)> {
    let weak_auto_destroy = options_weak_autodestroy(options);
    let flags = EventPoolAwaitFlags::from_bits_truncate(options);

    let int_disable = IntDisable::new();

    let event_pool = CapabilitySpace::current()
        .get_event_pool_with_perms(event_pool_id, CapFlags::WRITE, weak_auto_destroy)?
        .into_inner();

    let await_result = event_pool.await_event()?;

    drop(event_pool);

    match await_result {
        AwaitStatus::Success {
            event_range,
        } => {
            Ok((event_range.as_usize(), event_range.size()))
        },
        AwaitStatus::Block => {
            let post_switch_action = if flags.contains(EventPoolAwaitFlags::TIMEOUT) {
                PostSwitchAction::SetTimeout(timeout as u64)
            } else {
                PostSwitchAction::None
            };

            switch_current_thread_to(
                ThreadState::Suspended,
                int_disable,
                post_switch_action,
                false,
            ).expect("Failed to wait on event pool");

            match cpu_local_data().current_thread().wake_reason() {
                WakeReason::EventPoolEventRecieved { event_range } => {
                    Ok((event_range.as_usize(), event_range.size()))
                },
                WakeReason::Timeout => Err(SysErr::OkTimeout),
                _ => unreachable!(),
            }
        },
    }
}