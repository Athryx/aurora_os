mod linked_list_allocator;
mod pmem_allocator;
mod pmem_manager;
mod heap_allocator;
mod page_allocator;
mod fixed_page_allocator;
mod cap_allocator;
mod alloc_ref;

pub use heap_allocator::{HeapAllocator, AllocRef, OrigAllocator, OrigRef};
pub use page_allocator::{PageAllocator, PaRef};
pub use cap_allocator::CapAllocatorParent;

use spin::Once;

use crate::mb2::MemoryMap;
use pmem_manager::PmemManager;
use cap_allocator::RootAllocator;

static PMEM_MANAGER: Once<PmemManager> = Once::new();

// must call init before using
// panics if init has not been called
pub fn zm() -> &'static PmemManager {
	PMEM_MANAGER.get().expect("zone manager (PmemManager) has not been initilized")
}

static ROOT_ALLOCATOR: Once<RootAllocator> = Once::new();

pub fn root_allocator() -> &'static RootAllocator {
	ROOT_ALLOCATOR.get().expect("root allocator accessed before it was initilized")
}

// safety: must call before ever calling zm
pub unsafe fn init(mem_map: &MemoryMap) {
	unsafe {
		let mut total_pages = 0;
		PMEM_MANAGER.call_once(|| {
			let (pmem_manager, pages) = PmemManager::new(mem_map);
			total_pages = pages;
			pmem_manager
		});

		ROOT_ALLOCATOR.call_once(|| RootAllocator::new(total_pages));
	}
}
