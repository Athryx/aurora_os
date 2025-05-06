//! Functions related to memory managemant
//! 
//! This module includes the physical page allocator, the kernel heap, as well as the capability object allocator

mod cap_allocator;
mod fixed_page_allocator;
mod heap_allocator;
mod linked_list_allocator;
mod mmio_allocator;
mod page_allocator;
mod pmem_manager;

pub use cap_allocator::CapAllocator;
pub use heap_allocator::{HeapRef, HeapAllocator};
use linked_list_allocator::LinkedListAllocator;
pub use page_allocator::{PaRef, PageAllocator};
pub use mmio_allocator::{MmioAllocator, PhysMem};
use pmem_manager::PmemManager;
use spin::Once;

use crate::consts::KERNEL_PHYS_RANGE;
use crate::container::Arc;
use crate::mb2::MemoryMap;
use crate::prelude::*;


static PMEM_MANAGER: Once<PmemManager> = Once::new();

/// Returns the zone manager (which manages all physical pages on the system)
/// 
/// # Panics
/// Panics if the zone manager has not yet been initialized
pub fn zm() -> &'static PmemManager {
    PMEM_MANAGER
        .get()
        .expect("zone manager (PmemManager) has not been initilized")
}

static HEAP: Once<LinkedListAllocator> = Once::new();

/// Returns the kernel heap allocator
/// 
/// # Panics
/// Panics if the heap allocator has not yet been initilized
pub fn heap() -> &'static LinkedListAllocator {
    HEAP.get().expect("heap not yet initilized")
}

static ROOT_ALLOCATOR: Once<Arc<CapAllocator>> = Once::new();

/// Returns the root CapAllocator
/// 
/// # Panics
/// Panics if the root CapAllocator has not yet been intialized
pub fn root_alloc() -> &'static Arc<CapAllocator> {
    ROOT_ALLOCATOR
        .get()
        .expect("root allocator accessed before it was initilized")
}

/// Returns the root CapAllocator
/// 
/// # Panics
/// Panics if the root CapAllocator has not yet been intialized
pub fn root_alloc_ref() -> HeapRef {
    HeapRef::cap_allocator(root_alloc().clone().into())
}

/// Returns the root CapAllocator
/// 
/// # Panics
/// Panics if the root CapAllocator has not yet been intialized
pub fn root_alloc_page_ref() -> PaRef {
    PaRef::cap_allocator(root_alloc().clone().into())
}

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
            Arc::new(CapAllocator::new_root(total_pages), HeapRef::heap())
                .expect("failed to initilize root cap allocator")
        });

        let mut mmio_allocator = MmioAllocator::new(root_alloc_ref());
        for allocator in zm().allocers.iter() {
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
