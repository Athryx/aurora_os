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

use spin::Once;

use crate::mb2::MemoryMap;
use pmem_manager::PmemManager;

static PMEM_MANAGER: Once<PmemManager> = Once::new();

// must call init before using
// panics if init has not been called
pub fn zm() -> &'static PmemManager {
	PMEM_MANAGER.get().expect("zone manager (PmemManager) has not been initilized")
}

// safety: must call before ever calling zm
pub unsafe fn init(mem_map: &MemoryMap) {
	unsafe {
		PMEM_MANAGER.call_once(|| PmemManager::new(mem_map));
	}
}
