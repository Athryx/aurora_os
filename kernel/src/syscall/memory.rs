use crate::alloc::{PaRef, OrigRef};
use crate::cap::{StrongCapability, Capability};
use crate::cap::{CapFlags, memory::Memory};
use crate::prelude::*;
use crate::arch::x64::IntDisable;
use super::options_weak_autodestroy;

/// Allocate a memory capability at least `pages` big
/// 
/// returns the capability referencing the memory
/// 
/// # Options
/// bit 0-3 (mem_cap_flags): CapPriv representing privalidges over this memory
///
/// # Required Capability Permissions
/// `allocator`: cap_prod
///
/// # Returns
/// mem: cid of memory
pub fn memory_new(options: u32, allocator_id: usize, pages: usize) -> KResult<usize> {
    let weak_auto_destroy = options_weak_autodestroy(options);
    let mem_cap_flags = CapFlags::from_bits_truncate(get_bits(options as usize, 0..4));

    let _int_disable = IntDisable::new();

    let current_process = cpu_local_data().current_process();

    let allocator = current_process.cap_map()
        .get_allocator_with_perms(allocator_id, CapFlags::PROD, weak_auto_destroy)?;
    let page_allocator = PaRef::from_arc(allocator.clone());
    let heap_allocator = OrigRef::from_arc(allocator);

    let memory = StrongCapability::new(
        Memory::new(page_allocator, pages)?,
        mem_cap_flags,
        heap_allocator,
    )?;

    Ok(current_process.cap_map().insert_memory(Capability::Strong(memory))?.into())
}