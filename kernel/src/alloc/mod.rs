mod linked_list_allocator;
mod pmem_allocator;
mod pmem_manager;
mod heap_allocator;
mod page_allocator;
mod fixed_page_allocator;
mod alloc_ref;

pub use heap_allocator::{HeapAllocator, AllocRef, OrigAllocator, OrigRef};
pub use page_allocator::{PageAllocator, PaRef};

use crate::mb2::MemoryMap;
use pmem_manager::PmemManager;

// this is kind of ugly
static mut PMEM_MANAGER: Option<PmemManager> = None;

// must call init before using
pub fn zm() -> &'static PmemManager {
	// safety: init will be called at this point
	unsafe {
		PMEM_MANAGER.as_ref().unwrap()
	}
}

// safety: must call before ever calling zm
pub unsafe fn init(mem_map: &MemoryMap) {
	unsafe {
		PMEM_MANAGER = Some(PmemManager::new(mem_map));
	}
}
