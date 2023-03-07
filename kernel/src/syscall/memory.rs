use crate::alloc::{PaRef, OrigRef};
use crate::cap::{StrongCapability, Capability};
use crate::cap::{CapFlags, CapId, memory::Memory};
use crate::{prelude::*, process};
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

/// maps a capability `mem` that can be mapped into memory into the memory of process `process` starting at address `addr`
/// 
/// the cap id of `mem` is looked up in the process that is having memory mapped into it
/// 
/// the mapped memory read, write, and execute permissions depend on cap_read, cap_write, and cap_prod permissions respectively
/// will fail if `mem` overlaps with any other mapped memory
/// 
/// NOTE: weak_auto_destroy option does not currently apply to the memory capability
///
/// # Required Capability Permissions
/// `process`: cap_write
///
/// # Syserr Code
/// InvlOp: `mem` is already mapped into this process' address space
/// InvlVirtAddr: `addr` is non canonical
/// InvlAlign: `addr` is not page aligned
/// InvlMemZone: the value passed in for `addr` causes the mapped memory to overlap with other virtual memory
/// InvlWeak: `mem` is a weak capability, mapping a weak capability is not allowed
pub fn memory_map(
    options: u32,
    process_id: usize,
    memory_id: usize,
    addr: usize,
) -> KResult<()> {
    let weak_auto_destroy = options_weak_autodestroy(options);
    let addr = VirtAddr::try_new(addr).ok_or(SysErr::InvlVirtAddr)?;
    let memory_cap_id = CapId::try_from(memory_id).ok_or(SysErr::InvlId)?;

    let _int_disable = IntDisable::new();

    let process = cpu_local_data()
        .current_process()
        .cap_map()
        .get_process_with_perms(process_id, CapFlags::WRITE, weak_auto_destroy)?;

    process.map_memory(memory_cap_id, addr)
}

/// Unmaps memory mapped by [`memory_map`]
/// 
/// the cap id for `memory` is looked ip in the `process` argument, not the current process
/// 
/// NOTE: weak_auto_destroy option does not currently apply to the memory capability
///
/// # Required Capability Permissions
/// `process`: cap_write
///
/// # Syserr Code
/// InvlOp: `mem` is not mapped into `process` address space
/// InvlWeak: `mem` is a weak capability
pub fn memory_unmap(
    options: u32,
    process_id: usize,
    memory_id: usize,
) -> KResult<()> {
    let weak_auto_destroy = options_weak_autodestroy(options);
    let memory_cap_id = CapId::try_from(memory_id).ok_or(SysErr::InvlId)?;

    let _int_disable = IntDisable::new();

    let process = cpu_local_data()
        .current_process()
        .cap_map()
        .get_process_with_perms(process_id, CapFlags::WRITE, weak_auto_destroy)?;

    process.unmap_memory(memory_cap_id)
}