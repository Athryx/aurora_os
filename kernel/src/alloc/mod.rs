mod linked_list_allocator;
mod pmem_manager;
mod heap_allocator;
mod page_allocator;
mod fixed_page_allocator;
mod cap_allocator;
mod alloc_ref;

pub use heap_allocator::{HeapAllocator, AllocRef, OrigAllocator, OrigRef};
pub use page_allocator::{PageAllocator, PaRef};

use spin::Once;

use crate::prelude::*;
use crate::{mb2::MemoryMap, mem::Allocation};
use pmem_manager::PmemManager;
use linked_list_allocator::LinkedListAllocator;
use cap_allocator::CapAllocator;
use crate::container::Arc;


static PMEM_MANAGER: Once<PmemManager> = Once::new();

// must call init before using
// panics if init has not been called
pub fn zm() -> &'static PmemManager {
	PMEM_MANAGER.get().expect("zone manager (PmemManager) has not been initilized")
}

static HEAP: Once<LinkedListAllocator> = Once::new();

pub fn heap() -> &'static LinkedListAllocator {
	HEAP.get().expect("heap not yet initilized")
}

pub fn heap_ref() -> OrigRef {
	OrigRef::new(heap())
}

static ROOT_ALLOCATOR: Once<Arc<CapAllocator>> = Once::new();

pub fn root_alloc() -> &'static CapAllocator {
	ROOT_ALLOCATOR.get().expect("root allocator accessed before it was initilized")
}

pub fn root_alloc_ref() -> OrigRef {
	OrigRef::new(root_alloc())
}

// safety: must call before ever calling zm
pub unsafe fn init(mem_map: &MemoryMap) -> KResult<()> {
	unsafe {
		let mut total_pages = 0;
		PMEM_MANAGER.call_once(|| {
			let (pmem_manager, pages) = PmemManager::new(mem_map);
			total_pages = pages;
			pmem_manager
		});

		HEAP.call_once(|| {
			LinkedListAllocator::new(PaRef::new(zm()))
		});

		ROOT_ALLOCATOR.call_once(|| {
			Arc::new(CapAllocator::new_root(total_pages), heap_ref())
				.expect("failed to initilize root cap allocator")
		});

		Ok(())
	}
}
