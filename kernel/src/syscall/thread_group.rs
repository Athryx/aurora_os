use sys::CapFlags;

use crate::arch::x64::IntDisable;
use crate::cap::{Capability, StrongCapability};
use crate::cap::capability_space::CapabilitySpace;
use crate::mem::{HeapRef, PaRef};
use crate::prelude::*;
use crate::sched::ThreadGroup;
use super::options_weak_autodestroy;

pub fn thread_group_new(options: u32, parent_group_id: usize, allocator_id: usize) -> KResult<usize> {
    let weak_auto_destroy = options_weak_autodestroy(options);

    let _int_disable = IntDisable::new();

    let cspace = CapabilitySpace::current();

    let parent_group = cspace
        .get_thread_group_with_perms(parent_group_id, CapFlags::PROD, weak_auto_destroy)?
        .into_inner();

    let allocator = cspace
        .get_allocator_with_perms(allocator_id, CapFlags::PROD, weak_auto_destroy)?
        .into_inner();
    let heap_ref = HeapRef::from_arc(allocator.clone());
    let pa_ref = PaRef::from_arc(allocator);

    let new_thread_group = parent_group
        .create_child_thread_group(pa_ref, heap_ref)?;

    let thread_group_capability = StrongCapability::new_flags(
        new_thread_group,
        CapFlags::all(),
    );

    let threadad_group_cap_id = cspace.insert_thread_group(Capability::Strong(thread_group_capability))?;

    Ok(threadad_group_cap_id.into())
}

pub fn thread_group_exit(options: u32, thread_group_id: usize) -> KResult<()> {
    let weak_auto_destroy = options_weak_autodestroy(options);

    let _int_disable = IntDisable::new();

    let thread_group = CapabilitySpace::current()
        .get_thread_group_with_perms(thread_group_id, CapFlags::WRITE, weak_auto_destroy)?
        .into_inner();

    ThreadGroup::exit(thread_group);

    Ok(())
}