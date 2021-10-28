mod bump_allocator;
mod pmem_allocator;
mod pmem_manager;
mod heap_allocator;

pub use heap_allocator::{HeapAllocator, AllocRef};

use crate::mb2::MemoryMap;

pub fn init(mem_map: &MemoryMap) {
	unsafe {
		let tmp = pmem_manager::PmemManager::new(mem_map);
	}
}
