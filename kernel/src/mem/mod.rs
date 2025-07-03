// unless otherwise stated, all lens in this module are in bytes, not pages

pub mod page_allocator;
pub use page_allocator::{PaRef, PageAllocation, PageLayout};

pub mod heap_allocator;
pub use heap_allocator::{HeapRef};

pub mod vmem_manager;
pub mod tracking_allocator;
pub use tracking_allocator::{root_alloc_page_ref, root_alloc_ref, root_alloc};

pub mod mmio_allocator;
pub mod addr;
pub mod mem_owner;
pub mod range;

pub use addr::{phys_to_virt, virt_to_phys, PhysAddr, VirtAddr};
pub use mem_owner::{MemOwner, MemOwnerKernelExt};
pub use range::*;

use crate::mb2::MemoryMap;
use crate::prelude::*;
use crate::container::Arc;
use crate::consts::KERNEL_PHYS_RANGE;
use mmio_allocator::MmioAllocator;
use page_allocator::{PmemManager, PMEM_MANAGER, zm};
use heap_allocator::{LinkedListAllocator, HEAP};
use tracking_allocator::{TrackingAllocator, ROOT_ALLOCATOR};

/// Initilizes the memory allocation subsystem
/// 
/// # Safety
/// Must call with a valid memory map
pub unsafe fn init(mem_map: &MemoryMap) -> KResult<Arc<MmioAllocator>> {
    let mut total_pages = 0;
    PMEM_MANAGER.call_once(|| {
        let (pmem_manager, pages) = unsafe { PmemManager::new(mem_map) };
        total_pages = pages;
        pmem_manager
    });

    HEAP.call_once(|| LinkedListAllocator::new(PaRef::zm()));

    ROOT_ALLOCATOR.call_once(|| {
        Arc::new(TrackingAllocator::new_root(total_pages), HeapRef::heap())
            .expect("failed to initilize root cap allocator")
    });

    let mut mmio_allocator = MmioAllocator::new(root_alloc_ref());
    for allocator in zm().allocator_regions().iter() {
        let allocator_phys_range = allocator.addr_range.to_phys().as_aligned();
        mmio_allocator.add_reserved_region(allocator_phys_range)
            .expect("failed to reserve region for mmio allocator");
    }
    mmio_allocator.add_reserved_region(*KERNEL_PHYS_RANGE)
        .expect("failed to reserve kernel region for mmio allocator");

    Ok(Arc::new(
        mmio_allocator,
        root_alloc_ref(),
    )?)
}
