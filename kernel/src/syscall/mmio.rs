use sys::{CapFlags, MemoryMappingFlags};

use crate::alloc::HeapRef;
use crate::cap::{StrongCapability, Capability};
use crate::cap::capability_space::CapabilitySpace;
use crate::prelude::*;
use crate::arch::x64::IntDisable;
use crate::container::Arc;
use crate::vmem_manager::PageMappingOptions;

use super::options_weak_autodestroy;

pub fn mmio_allocator_alloc(options: u32, mmio_allocator_id: usize, allocator_id: usize, phys_address: usize, page_count: usize) -> KResult<usize> {
    let weak_auto_destroy = options_weak_autodestroy(options);
    let phys_address = PhysAddr::try_new(phys_address)
        .ok_or(SysErr::InvlPhysAddr)?;

    let size = Size::try_from_pages(page_count)
        .ok_or(SysErr::Overflow)?;

    let phys_range = APhysRange::try_new_aligned(phys_address, size.bytes())
        .ok_or(SysErr::InvlAlign)?;

    let _int_disable = IntDisable::new();

    let cspace = CapabilitySpace::current();

    let mmio_allocator = cspace
        .get_mmio_allocator_with_perms(mmio_allocator_id, CapFlags::PROD, weak_auto_destroy)?
        .into_inner();

    let allocator = cspace
        .get_allocator_with_perms(allocator_id, CapFlags::PROD, weak_auto_destroy)?
        .into_inner();
    let heap_ref = HeapRef::from_arc(allocator);

    let phys_mem = mmio_allocator.alloc(phys_range)?;
    let phys_mem_cap = StrongCapability::new_flags(
        Arc::new(
            phys_mem,
            heap_ref,
        )?,
        CapFlags::all(),
    );

    let cap_id = cspace.insert_phys_mem(Capability::Strong(phys_mem_cap))?;
    Ok(cap_id.into())
}

pub fn phys_mem_map(options: u32, addr_space_id: usize, phys_mem_id: usize, address: usize) -> KResult<usize> {
    let weak_auto_destroy = options_weak_autodestroy(options);

    let map_flags = MemoryMappingFlags::from_bits_truncate(options);
    let map_options = PageMappingOptions::from(map_flags);

    let address = VirtAddr::try_new_aligned(address)?;

    let _int_disable = IntDisable::new();

    let cspace = CapabilitySpace::current();

    let addr_space = cspace
        .get_address_space_with_perms(addr_space_id, CapFlags::WRITE, weak_auto_destroy)?
        .into_inner();

    let phys_mem = cspace
        .get_phys_mem_with_perms(phys_mem_id, map_options.required_cap_flags(), weak_auto_destroy)?
        .into_inner();

    phys_mem.map(&addr_space, address, map_options)
        .map(Size::pages_rounded)
}

pub fn phys_mem_get_size(options: u32, phys_mem_id: usize) -> KResult<usize> {
    let weak_auto_destroy = options_weak_autodestroy(options);

    let _int_disable = IntDisable::new();

    let phys_mem = CapabilitySpace::current()
        .get_phys_mem_with_perms(phys_mem_id, CapFlags::READ, weak_auto_destroy)?
        .into_inner();

    Ok(phys_mem.size().pages_rounded())
}