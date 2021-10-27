use crate::prelude::*;
use crate::mb2::{MemoryMap, MemoryRegionType};
use super::pmem_allocator::PmemAllocator;
use super::bump_allocator::BumpAllocator;
use super::{HeapAllocator, AllocRef};

pub struct PmemManager {
	allocers: *const [PmemAllocator],
}

impl PmemManager {
	// TODO: this might encounter problems with low amount of system memory (like very low)
	pub unsafe fn new(mem_map: &MemoryMap) -> PmemManager {
		let usable = mem_map.iter()
			.filter(|zone| matches!(zone, MemoryRegionType::Usable(_)));

		// phys range
		let max = usable.clone()
			.reduce(|z1, z2| if z1.range().size() > z2.range().size() {
				z1
			} else {
				z2
			}).unwrap().range();

		// get the size of the largest power of 2 aligned chunk of memory
		// we will use this memory for the temporary bump allocator to store heap data needed to set up buddy allocators
		// use the biggest chunk because smaller chunks will be used for allocator metadata,
		// but the biggest chunk will always be selected as allocatable memory, so it won't be written to during inititilization
		let mut level_size = 1 << log2(max.size());
		let mut level_addr = align_up(max.as_usize(), level_size);
		if level_addr + level_size > max.end_usize() {
			level_size >>= 1;
			level_addr = align_up(max.as_usize(), level_size);
		}

		let paddr = PhysAddr::new(level_addr);
		let vrange = VirtRange::new_unaligned(paddr.to_virt(), level_size);

		// make new bump allocator to use for initializing physical memory ranges
		let allocer = BumpAllocator::new(vrange);
		let temp = &allocer as *const dyn HeapAllocator;
		let aref = AllocRef::new_raw(temp);

		for regions in usable {
		}
		todo!();
	}
}
