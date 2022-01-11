use core::cmp::min;
use core::alloc::Layout;

use crate::prelude::*;
use crate::mb2::{MemoryMap, MemoryRegionType};
use crate::container::VecMap;
use super::pmem_allocator::PmemAllocator;
use super::linked_list_allocator::LinkedListAllocator;
use super::fixed_page_allocator::FixedPageAllocator;
use super::{PageAllocator, PaRef, OrigAllocator, OrigRef};

struct PmemInitMap {
	// all zones that can be allocatable memory and metadata
	zones: VecMap<usize, UVirtRange>,
	// zones that are too big and no other zone can hold their metadata, or zones that are no longer size aligned
	// if this is not empty, it is used for allocating metadata
	nofit: VecMap<usize, UVirtRange>,
}

impl PmemInitMap {
	fn new(zones: VecMap<usize, UVirtRange>, nofit: VecMap<usize, UVirtRange>) -> Self {
		PmemInitMap {
			zones,
			nofit,
		}
	}

	fn get_mem_zone(&mut self) -> Option<UVirtRange> {
		self.zones.pop_max().map(|data| data.1)
	}

	fn get_slice<T>(&mut self, len: usize) -> &[T] {
		let size = len * size_of::<T>();
		loop {
			match self.nofit.remove_gt(&size) {
				Some(range) => {
				},
				None => {
				},
			}
		}
	}
}

pub struct PmemManager {
	allocers: *const [PmemAllocator],
}

impl PmemManager {
	// TODO: this might encounter problems with low amount of system memory (like very low)
	pub unsafe fn new(mem_map: &MemoryMap) -> PmemManager {
		// iterator over usable memory zones as a VirtRange
		let usable = mem_map.iter()
			.filter(|zone| matches!(zone, MemoryRegionType::Usable(_)))
			.map(|mem| mem.range().to_virt());

		// biggest usable virt range
		// align to pages because we will use this for the initial allocator
		let max = usable.clone()
			.reduce(|z1, z2| if z1.size() > z2.size() {
				z1
			} else {
				z2
			}).unwrap().as_inside_aligned().unwrap();

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

		// Panic safety: will be aligned because level_addr and level_size are aligned in above code
		let vrange = AVirtRange::new(VirtAddr::new(level_addr), level_size);

		// A fixed page allocator used as the initial page allocator
		// panic safety: this range is the biggest range, it should not fail
		let page_allocator = FixedPageAllocator::new(vrange);
		let pa_ptr = &page_allocator as *const dyn PageAllocator;
		let page_ref = unsafe { PaRef::new_raw(pa_ptr) };

		let allocer = LinkedListAllocator::new(page_ref);
		let temp = &allocer as *const dyn OrigAllocator;
		// Safety: make sure not to use this outside of this function
		let aref = unsafe {
			OrigRef::new_raw(temp)
		};

		// maximum number of level zones that could exist
		// TODO: find out how to actually calculate this
		let max_zones = usable.clone()
			.fold(0, |acc, range| {
				acc + 2 * log2(range.size()) - 1
			});

		// holds zones of memory that have a size of power of 2 and an alignmant equal to their size
		// TODO: maybe use a better data structure than vec
		// because some elements are removed from the middle, vec is not an optimal data structure,
		// but it is the only one written at the moment, and this code is run once and is not performance critical
		let mut zones = VecMap::try_with_capacity(aref.downgrade(), max_zones)
			.expect("not enough memory to initialize physical memory manager");

		// zones that don't have any other zone that can hold all of their metadata
		//let mut nofit = VecMap::new(aref);

		for region in usable {
			let mut start = region.as_usize();
			let end = region.end_usize();

			while start < end {
				let size = min(align_of(start), end - start);
				// because region is aligned, this should be aligned
				let range = UVirtRange::new(VirtAddr::new(start), size);
				zones.insert(range.size(), range).expect("vec was not made big enough");

				start += size;
			}
		}

		// get slice of memory to hold PmemAllocators
		// not optimal prediction of how many allocators there will be, but good enough
		let size = zones.len() * size_of::<PmemAllocator>();


		while let Some((_, max)) = zones.pop_max() {
		}

		todo!();
	}
}
