use core::cmp::min;

use crate::prelude::*;
use crate::mb2::{MemoryMap, MemoryRegionType};
use crate::container::Vec;
use super::pmem_allocator::PmemAllocator;
use super::bump_allocator::BumpAllocator;
use super::{HeapAllocator, AllocRef};

pub struct PmemManager {
	allocers: *const [PmemAllocator],
}

impl PmemManager {
	// TODO: this might encounter problems with low amount of system memory (like very low)
	pub unsafe fn new(mem_map: &MemoryMap) -> PmemManager {
		// iterator over usable memory zones as a VirtRange
		// these ranges are aligned on pages
		let usable = mem_map.iter()
			.filter(|zone| matches!(zone, MemoryRegionType::Usable(_)))
			.map(|mem| mem.range().to_virt().aligned());

		// biggest usable virt range
		let max = usable.clone()
			.reduce(|z1, z2| if z1.size() > z2.size() {
				z1
			} else {
				z2
			}).unwrap();

		// maximum number of level zones that could exist
		let max_zones = usable.clone()
			.fold(0, |acc, range| {
				acc + 2 * log2(range.size()) - 1
			});

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

		// will be aligned because level_addr and level_size are aligned in above code
		let vrange = VirtRange::new_unaligned(VirtAddr::new(level_addr), level_size);

		// make new bump allocator to use for initializing physical memory ranges
		let allocer = BumpAllocator::new(vrange);
		let temp = &allocer as *const dyn HeapAllocator;
		let aref = AllocRef::new_raw(temp);

		// holds zones of memory that have a size of power of 2 and an alignmant equal to their size
		// TODO: maybe use a better data structure than vec
		// because some elements are removed from the middle, vec is not an optimal data structure,
		// but it is the only one written at the moment, and this code is run once and is not performance critical
		let mut zones = Vec::try_with_capacity(aref, max_zones).expect("not enough memory to initialize physical memory manager");

		for region in usable {
			let mut start = region.as_usize();
			let end = region.end_usize();

			while start < end {
				let size = min(align_of(start), end - start);
				// because region is aligned, this should be aligned
				let range = VirtRange::new_unaligned(VirtAddr::new(start), size);
				zones.push(range).expect("vec was not made big enough");

				start += size;
			}
		}

		zones.sort_unstable();

		let find_range = |vec: &mut Vec<VirtRange>, size: usize| -> Option<VirtRange> {
			let bsearch_by = |probe: &VirtRange| probe.size().cmp(&size);
			let i = match vec.binary_search_by(bsearch_by) {
				Ok(i) => i,
				Err(i) => i,
			};

			if i >= vec.len() {
				None
			} else {
				Some(vec.remove(i))
			}
		};

		while let Some(max) = zones.pop() {
			let range = find_range(&mut zones, max.size());
		}

		todo!();
	}
}
