use crate::alloc::{PaRef, OrigRef};
use crate::cap::{StrongCapability, Capability};
use crate::cap::{CapFlags, CapId, memory::Memory};
use crate::process::PageMappingFlags;
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
/// # Syserr code
/// InvlArgs: value for `pages` was 0, 0 sized memory is not allowed
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
/// NOTE: weak auto destroy does not apply to the `mem` capability
/// 
/// # Options
/// bit 0 (mem_read): the mapped memory region shold be readable (requires read permissions on memory capability)
/// bit 1 (mem_write): the mapped memory region should be writable (requires write permissions on memory capability)
/// bit 2 (mem_exec): the mapped memory region should be executable (requires read permissions on memory capability)
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
/// InvlArgs: options has no bits set indicating read, write, or exec permissions
/// 
/// # Returns
/// size: size of the memory that was mapped into address space (this will be the size of memory capability)
pub fn memory_map(
    options: u32,
    process_id: usize,
    memory_id: usize,
    addr: usize,
) -> KResult<usize> {
    let weak_auto_destroy = options_weak_autodestroy(options);
    let addr = VirtAddr::try_new(addr).ok_or(SysErr::InvlVirtAddr)?;
    let memory_cap_id = CapId::try_from(memory_id).ok_or(SysErr::InvlId)?;

    let _int_disable = IntDisable::new();

    let process = cpu_local_data()
        .current_process()
        .cap_map()
        .get_process_with_perms(process_id, CapFlags::WRITE, weak_auto_destroy)?;

    let map_flags = PageMappingFlags::from_bits_truncate((options & 0b111) as usize);

    let mut required_cap_flags = CapFlags::empty();
    if map_flags.contains(PageMappingFlags::READ | PageMappingFlags::EXEC) {
        required_cap_flags |= CapFlags::READ;
    }
    if map_flags.contains(PageMappingFlags::WRITE) {
        required_cap_flags |= CapFlags::WRITE;
    }

    if !memory_cap_id.flags().contains(required_cap_flags) {
        return Err(SysErr::InvlPerm);
    }

    process.map_memory(memory_cap_id, addr, map_flags)
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

/// Resizes the memory capability referenced by `memory`
/// 
/// `memory` must not be mapped anywhere in memory
/// 
/// # Required Capability Permissions
/// `memory`: cap_prod
/// 
/// # Syserr Code
/// InvlOp: `memory` is mapped into memory somewhere
/// InvlArgs: `new_page_size` is 0
pub fn memory_resize(
    options: u32,
    memory_id: usize,
    new_page_size: usize,
) -> KResult<()> {
    let weak_auto_destroy = options_weak_autodestroy(options);

    let _int_disable = IntDisable::new();

    let memory = cpu_local_data()
        .current_process()
        .cap_map()
        .get_memory_with_perms(memory_id, CapFlags::PROD, weak_auto_destroy)?;

    let mut memory_inner = memory.inner();

    if memory_inner.map_ref_count == 0 {
        // Safety: map ref count is checked to be 0
        unsafe {
            memory_inner.resize(new_page_size)
        }
    } else {
        Err(SysErr::InvlOp)
    }
}