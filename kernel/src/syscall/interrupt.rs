use sys::{CapFlags, InterruptTrigger};

use crate::alloc::HeapRef;
use crate::cap::{Capability, StrongCapability};
use crate::cap::capability_space::CapabilitySpace;
use crate::container::Arc;
use crate::int::userspace_interrupt::Interrupt;
use crate::prelude::*;
use crate::arch::x64::IntDisable;
use super::options_weak_autodestroy;

pub fn interrupt_new(options: u32, int_allocator_id: usize, allocator_id: usize) -> KResult<(usize, usize, usize)> {
    let weak_auto_destroy = options_weak_autodestroy(options);

    let _int_disable = IntDisable::new();

    let cspace = CapabilitySpace::current();

    let _int_allocator = cspace
        .get_int_allocator_with_perms(int_allocator_id, CapFlags::PROD, weak_auto_destroy)?;

    let allocator = cspace
        .get_allocator_with_perms(allocator_id, CapFlags::PROD, weak_auto_destroy)?
        .into_inner();
    let allocator = HeapRef::from_arc(allocator);

    let interrupt = Interrupt::new(&allocator)?;
    let interrupt_id = interrupt.interrupt_id();

    let int_capability = StrongCapability::new_flags(
        Arc::new(interrupt, allocator)?,
        CapFlags::all(),
    );

    let cap_id = cspace.insert_interrupt(Capability::Strong(int_capability))?;

    Ok((
        cap_id.into(),
        interrupt_id.cpu.into(),
        interrupt_id.interrupt_num as usize,
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