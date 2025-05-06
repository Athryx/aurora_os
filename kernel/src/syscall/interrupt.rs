use sys::{CapFlags, InterruptTrigger};

use crate::alloc::HeapRef;
use crate::cap::StrongCapability;
use crate::cap::capability_space::CapabilitySpace;
use crate::container::Arc;
use crate::int::userspace_interrupt::{interrupt_manager, Interrupt};
use crate::prelude::*;
use crate::arch::x64::IntDisable;
use super::options_weak_autodestroy;

pub fn interrupt_new(
    options: u32,
    int_allocator_id: usize,
    allocator_id: usize,
    interrupt_count: usize,
    interrupt_align: usize,
) -> KResult<(usize, usize, usize)> {
    let weak_auto_destroy = options_weak_autodestroy(options);

    let _int_disable = IntDisable::new();

    let cspace = CapabilitySpace::current();

    let _int_allocator = cspace
        .get_int_allocator_with_perms(int_allocator_id, CapFlags::PROD, weak_auto_destroy)?;

    let allocator = cspace
        .get_allocator_with_perms(allocator_id, CapFlags::PROD, weak_auto_destroy)?
        .into_inner();
    let allocator = HeapRef::from_arc(allocator);

    // find region of interrupts to use for userspace interrupts
    let interrupt_iter = interrupt_manager().alloc_interrupts(interrupt_count, interrupt_align)?;
    let base_interrupt_id = interrupt_iter.base_interrupt_id();

    let interrupt_capability_iter = interrupt_iter.map(|interrupt| Ok(StrongCapability::new_flags(
        Arc::new(interrupt, allocator.clone())?,
        CapFlags::all(),
    )));

    let base_cap_id = cspace.insert_interrupt_multiple(interrupt_capability_iter, CapFlags::all())?;

    Ok((
        base_cap_id.into(),
        base_interrupt_id.cpu.into(),
        base_interrupt_id.interrupt_num as usize,
    ))
}

/// Gets the interrupt id for a given interrupt
pub fn interrupt_id(options: u32, interrupt_id: usize) -> KResult<(usize, usize)> {
    let weak_auto_destroy = options_weak_autodestroy(options);

    let _int_disable = IntDisable::new();

    let interrupt_id = CapabilitySpace::current()
        .get_interrupt_with_perms(interrupt_id, CapFlags::READ, weak_auto_destroy)?
        .into_inner()
        .interrupt_id();

    Ok((
        interrupt_id.cpu.into(),
        interrupt_id.interrupt_num as usize,
    ))
}

crate::generate_event_syscall!(interrupt, InterruptTrigger, interrupt_trigger, CapFlags::PROD, Interrupt::add_interrupt_listener);