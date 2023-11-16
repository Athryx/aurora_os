use sys::{MemoryNewFlags, MemoryResizeFlags, MemoryMapFlags, MemoryUpdateMappingFlags};

use crate::alloc::{PaRef, HeapRef};
use crate::cap::address_space::AddressSpace;
use crate::cap::capability_space::CapabilitySpace;
use crate::cap::memory::{PageSource, MapMemoryArgs, UpdateValue, UpdateMappingAgs};
use crate::cap::{StrongCapability, Capability};
use crate::cap::{CapFlags, memory::Memory};
use crate::vmem_manager::PageMappingFlags;
use crate::prelude::*;
use crate::arch::x64::IntDisable;
use crate::container::Arc;
use super::options_weak_autodestroy;

pub fn address_space_new(options: u32, allocator_id: usize) -> KResult<usize> {
    let weak_auto_destroy = options_weak_autodestroy(options);

    let _int_disable = IntDisable::new();

    let cspace = CapabilitySpace::current();

    let allocator = cspace
        .get_allocator_with_perms(allocator_id, CapFlags::PROD, weak_auto_destroy)?
        .into_inner();
    let page_allocator = PaRef::from_arc(allocator.clone());
    let heap_allocator = HeapRef::from_arc(allocator);

    let address_space = StrongCapability::new_flags(
        Arc::new(
            AddressSpace::new(page_allocator, heap_allocator.clone())?,
            heap_allocator,
        )?,
        CapFlags::all(),
    );

    let cap_id = cspace.insert_address_space(Capability::Strong(address_space))?;

    Ok(cap_id.into())
}

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
/// size: size of the new memory capability in pages
pub fn memory_new(options: u32, allocator_id: usize, pages: usize) -> KResult<(usize, usize)> {
    let weak_auto_destroy = options_weak_autodestroy(options);
    let flags = MemoryNewFlags::from_bits_truncate(options);

    let page_source = match (flags.contains(MemoryNewFlags::LAZY_ALLOC), flags.contains(MemoryNewFlags::ZEROED)) {
        (false, false) => PageSource::Owned,
        (false, true) => PageSource::OwnedZeroed,
        (true, false) => PageSource::LazyAlloc,
        (true, true) => PageSource::LazyZeroAlloc,
    };

    let _int_disable = IntDisable::new();

    let cspace = CapabilitySpace::current();

    let allocator = cspace
        .get_allocator_with_perms(allocator_id, CapFlags::PROD, weak_auto_destroy)?
        .into_inner();
    let page_allocator = PaRef::from_arc(allocator.clone());
    let heap_allocator = HeapRef::from_arc(allocator);

    let memory = StrongCapability::new_flags(
        Arc::new(
            Memory::new_with_page_source(page_allocator, heap_allocator.clone(), pages, page_source)?,
            heap_allocator,
        )?,
        CapFlags::all(),
    );

    let size = memory.inner().inner_read().size();

    Ok((cspace.insert_memory(Capability::Strong(memory))?.into(), size.pages_rounded()))
}

/// Get the size of the memory capability in pages
/// 
/// # Required Capability Permissions
/// `memory`: cap_read
/// 
/// # Returns
/// size: size of memory capid in pages
pub fn memory_get_size(options: u32, memory_id: usize) -> KResult<usize> {
    let weak_auto_destroy = options_weak_autodestroy(options);

    let _int_disable = IntDisable::new();

    let memory = CapabilitySpace::current()
        .get_memory_with_perms(memory_id, CapFlags::READ, weak_auto_destroy)?
        .into_inner();

    let inner = memory.inner_read();

    Ok(inner.size().pages_rounded())
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
/// bit 3 (mem_max_size): the mapped memory region will be no larger than `max_size` pages large, instead of being the size of the capability by default
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
/// size: size of the memory that was mapped into address space in pages
pub fn memory_map(
    options: u32,
    addr_space_id: usize,
    memory_id: usize,
    addr: usize,
    max_size: usize,
    offset: usize,
) -> KResult<usize> {
    let weak_auto_destroy = options_weak_autodestroy(options);
    let addr = VirtAddr::try_new_aligned(addr)?;

    let map_flags = PageMappingFlags::from_bits_truncate((options & 0b111) as usize)
        | PageMappingFlags::USER;
    let other_flags = MemoryMapFlags::from_bits_truncate(options);

    let max_size = if other_flags.contains(MemoryMapFlags::MAX_SIZE) {
        let size = Size::try_from_pages(max_size)
            .ok_or(SysErr::Overflow)?;

        Some(size)
    } else {
        None
    };

    let offset = Size::try_from_pages(offset).ok_or(SysErr::Overflow)?;

    let mut required_cap_flags = CapFlags::empty();
    if map_flags.contains(PageMappingFlags::READ | PageMappingFlags::EXEC) {
        required_cap_flags |= CapFlags::READ;
    }
    if map_flags.contains(PageMappingFlags::WRITE) {
        required_cap_flags |= CapFlags::WRITE;
    }

    let _int_disable = IntDisable::new();

    let cspace = CapabilitySpace::current();

    let addr_space = cspace
        .get_address_space_with_perms(addr_space_id, CapFlags::WRITE, weak_auto_destroy)?
        .into_inner();

    let memory = cspace
        .get_memory_with_perms(memory_id, required_cap_flags, weak_auto_destroy)?
        .into_inner();

    Memory::map_memory(memory, addr_space, MapMemoryArgs {
        map_addr: addr,
        map_size: max_size,
        offset,
        flags: map_flags,
    }).map(Size::pages_rounded)
}

/// Unmaps memory mapped by [`memory_map`]
/// 
/// the cap id for `memory` is looked up in the `process` argument, not the current process
/// 
/// NOTE: weak_auto_destroy option does not currently apply to the memory capability
///
/// # Required Capability Permissions
/// `process`: cap_write
///
/// # Syserr Code
/// InvlOp: `mem` is not mapped into `process` address space
/// InvlWeak: `mem` is a weak capability
// FIXME: make work with event pool and rename
pub fn memory_unmap(
    options: u32,
    addr_space_id: usize,
    address: usize,
) -> KResult<()> {
    let weak_auto_destroy = options_weak_autodestroy(options);
    let address = VirtAddr::try_new_aligned(address)?;

    let _int_disable = IntDisable::new();

    let addr_space = CapabilitySpace::current()
        .get_address_space_with_perms(addr_space_id, CapFlags::WRITE, weak_auto_destroy)?
        .into_inner();

    let memory = addr_space.memory_at_addr(address)?;

    memory.unmap_memory(&addr_space, address)
}

/// Updates memory mappings created by [`memory_map`]
/// 
/// the cap id for `memory` is looked up in the `process` argument, not the current process
/// 
/// NOTE: weak_auto_destroy option does not currently apply to the memory capability
/// 
/// # Options
/// bit 0 (memory_update_size): change the mappings size to `new_page_size`, otherwise leave it unchanged
///
/// # Required Capability Permissions
/// `process`: cap_write
///
/// # Syserr Code
/// InvlOp: `mem` is not mapped into `process` address space
/// InvlWeak: `mem` is a weak capability
/// 
/// # Returns
/// Returns the size of the new mapping in pages
pub fn memory_update_mapping(
    options: u32,
    addr_space_id: usize,
    address: usize,
    new_page_size: usize,
) -> KResult<usize> {
    let weak_auto_destroy = options_weak_autodestroy(options);
    let map_flags = PageMappingFlags::from_bits_truncate((options & 0b111) as usize)
        | PageMappingFlags::USER;
    let other_flags = MemoryUpdateMappingFlags::from_bits_truncate(options);

    let address = VirtAddr::try_new_aligned(address)?;

    let size = if other_flags.contains(MemoryUpdateMappingFlags::UPDATE_SIZE) {
        let new_size = if other_flags.contains(MemoryUpdateMappingFlags::EXACT_SIZE) {
            Some(Size::try_from_pages(new_page_size).ok_or(SysErr::Overflow)?)
        } else {
            None
        };

        UpdateValue::Change(new_size)
    } else {
        UpdateValue::KeepSame
    };

    let flags = if other_flags.contains(MemoryUpdateMappingFlags::UPDATE_FLAGS) {
        UpdateValue::Change(map_flags)
    } else {
        UpdateValue::KeepSame
    };

    let _int_disable = IntDisable::new();

    let addr_space = CapabilitySpace::current()
        .get_address_space_with_perms(addr_space_id, CapFlags::WRITE, weak_auto_destroy)?
        .into_inner();

    let memory = addr_space.memory_at_addr(address)?;

    memory.update_mapping(&addr_space, address, UpdateMappingAgs {
        size,
        flags,
    }).map(Size::pages_rounded)
}

/// Resizes the memory capability referenced by `memory`
/// 
/// `memory` must not be mapped anywhere in memory, unless `mem_resize_in_place` is set
/// 
/// NOTE: weak auto destroy does not apply to the `mem` capability
/// 
/// # Options
/// bit 0 (mem_resize_in_place): allows the memory to be resived even if it is mapped in memory
/// as long as the only capability for which it is mapped in memory is `memory`
/// bit 1 (mem_resize_grow_mapping): if the memory is grown while mapped with the resize in place bit,
/// the mapping is automatically grown to be the new size of the entire mapping
/// 
/// # Required Capability Permissions
/// `memory`: cap_prod
/// 
/// # Syserr Code
/// InvlOp: `memory` is mapped into memory somewhere when it shouldn't be
/// InvlArgs: `new_page_size` is 0
/// 
/// # Returns
/// The new size of the memory capability in pages
pub fn memory_resize(
    options: u32,
    memory_id: usize,
    new_page_size: usize,
) -> KResult<usize> {
    let weak_auto_destroy = options_weak_autodestroy(options);
    let flags = MemoryResizeFlags::from_bits_truncate(options);

    let page_source = match (flags.contains(MemoryResizeFlags::LAZY_ALLOC), flags.contains(MemoryResizeFlags::ZEROED)) {
        (false, false) => PageSource::Owned,
        (false, true) => PageSource::OwnedZeroed,
        (true, false) => PageSource::LazyAlloc,
        (true, true) => PageSource::LazyZeroAlloc,
    };

    let new_page_size = Size::try_from_pages(new_page_size)
        .ok_or(SysErr::Overflow)?;

    let _int_disable = IntDisable::new();

    let cspace = CapabilitySpace::current();

    let memory = cspace
        .get_memory_with_perms(memory_id, CapFlags::PROD, weak_auto_destroy)?
        .into_inner();

    if flags.contains(MemoryResizeFlags::IN_PLACE) {
        memory.resize_in_place(
            new_page_size,
            flags.contains(MemoryResizeFlags::GROW_MAPPING),
            page_source,
        )
    } else {
        memory.resize(new_page_size, page_source)
    }.map(Size::pages_rounded)
}