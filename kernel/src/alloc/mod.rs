mod bump_allocator;
// mod linked_list_allocator;
mod pmem_allocator;
mod pmem_manager;
mod heap_allocator;
mod page_allocator;
mod fixed_page_allocator;

pub use heap_allocator::{HeapAllocator, AllocRef};
pub use page_allocator::{PageAllocator, PaRef};

use crate::mb2::MemoryMap;

pub fn init(mem_map: &MemoryMap) {
	unsafe {
		let tmp = pmem_manager::PmemManager::new(mem_map);
	}
}
