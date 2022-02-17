use core::mem::MaybeUninit;
use core::cmp::min;
use core::alloc::Layout;
use core::slice;
use core::sync::atomic::{AtomicU8, AtomicUsize, Ordering};

use crate::prelude::*;
use crate::mb2::{MemoryMap, MemoryRegionType};
use crate::container::VecMap;
use crate::mem::{Allocation, PageLayout};
use super::pmem_allocator::PmemAllocator;
use super::linked_list_allocator::LinkedListAllocator;
use super::fixed_page_allocator::FixedPageAllocator;
use super::{PageAllocator, PaRef, OrigAllocator, OrigRef};

// metadata range
enum MetaRange {
	Main(usize, AVirtRange),
	Meta(usize, UVirtRange),
}

type MainMap = VecMap<usize, AVirtRange>;
type MetaMap = VecMap<usize, UVirtRange>;


impl MetaRange {
	fn new(size: usize, main_map: &mut MainMap, meta_map: &mut MetaMap) -> Option<Self> {
		match meta_map.remove_gt(&size) {
			Some((size, range)) => Some(MetaRange::Meta(size, range)),
			None => {
				match main_map.remove_gt(&size) {
					Some((size, range)) => Some(MetaRange::Main(size, range)),
					None => None,
				}
			},
		}
	}

	fn insert_into(&self, main_map: &mut MainMap, meta_map: &mut MetaMap) {
		match *self {
			Self::Main(size, range) => {
				main_map.insert(size, range).unwrap();
			},
			Self::Meta(size, range) => {
				meta_map.insert(size, range).unwrap();
			},
		}
	}

	fn range(&self) -> UVirtRange {
		match self {
			Self::Main(_, range) => range.as_unaligned(),
			Self::Meta(_, range) => *range,
		}
	}
}

pub struct PmemManager {
	allocers: &'static [PmemAllocator],
	next_index: AtomicUsize,
}

impl PmemManager {
	// TODO: this might encounter problems with low amount of system memory (like very low)
	pub unsafe fn new(mem_map: &MemoryMap) -> PmemManager {
		// iterator over usable memory zones as a VirtRange
		let usable = mem_map.iter()
			.filter(|zone| matches!(zone, MemoryRegionType::Usable(_)))
			.filter_map(|mem| mem.range().to_virt().as_inside_aligned());

		// biggest usable virt range
		// align to pages because we will use this for the initial allocator
		let max = usable.clone()
			.reduce(|z1, z2| if z1.size() > z2.size() {
				z1
			} else {
				z2
			})
			.expect("no usable memory zones found");

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
		let init_heap_vrange = AVirtRange::new(VirtAddr::new(level_addr), level_size);

		// A fixed page allocator used as the initial page allocator
		// panic safety: this range is the biggest range, it should not fail
		let page_allocator = unsafe { FixedPageAllocator::new(init_heap_vrange) };
		let pa_ptr = &page_allocator as *const dyn PageAllocator;
		let page_ref = unsafe {
			PaRef::new_raw(pa_ptr)
		};

		let allocer = LinkedListAllocator::new(page_ref);
		let temp = &allocer as *const dyn OrigAllocator;
		// Safety: make sure not to use this outside of this function
		let aref = unsafe {
			OrigRef::new_raw(temp)
		};

		// maximum number of level zones that could exist
		// TODO: find out how to actually calculate this, but this is good enough for now
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

		// holds zones taken from zones vecmap that are used to store metadata
		let mut metadata_zones: VecMap<usize, UVirtRange> = VecMap::new(aref.downgrade());

		for region in usable {
			let mut start = region.as_usize();
			let end = region.end_usize();

			while start < end {
				let size = min(align_of(start), 1 << log2(end - start));
				// because region is aligned, this should be aligned
				let range = AVirtRange::new(VirtAddr::new(start), size);
				zones.insert(range.size(), range).expect("vec was not made big enough");

				start += size;
			}
		}

		// one zone will be used to store the allocators
		let allocator_count = zones.len() - 1;

		// get slice of memory to hold PmemAllocators
		// not optimal prediction of how many allocators there will be, but there can't be more
		let size = allocator_count * size_of::<PmemAllocator>();

		// get a region of memory to store all of the allocators
		let (_, orig_allocator_range) = zones.remove_gt(&size).unwrap();
		let mut orig_allocator_range = orig_allocator_range.as_unaligned();

		assert!(!init_heap_vrange.contains_range(&orig_allocator_range), "tried to use memory range for allocator initilizer heap to store allocator objects");

		// only get part that is needed to store all allocator objects
		let allocator_range = orig_allocator_range.take_layout(Layout::array::<PmemAllocator>(allocator_count).unwrap()).unwrap();

		// store the other part in the metadata array
		if orig_allocator_range.size() != 0 {
			metadata_zones.insert(orig_allocator_range.size(), orig_allocator_range).unwrap();
		}

		let allocator_slice = unsafe {
			slice::from_raw_parts_mut(allocator_range.as_usize() as *mut MaybeUninit<PmemAllocator>, allocator_count)
		};

		// index of current allocator
		let mut i = 0;

		while let Some((current_size, current_zone)) = zones.pop_max() {
			let (unaligned_tree_size, index_size) = PmemAllocator::required_tree_index_size(current_zone, PAGE_SIZE).unwrap();
			let tree_size = align_up(unaligned_tree_size, size_of::<usize>());

			let tree_data = if let Some(data) = MetaRange::new(tree_size, &mut zones, &mut metadata_zones) {
				data
			} else {
				// give up on using this zone, use it for metadata instead
				metadata_zones.insert(current_size, current_zone.as_unaligned()).unwrap();
				continue;
			};

			let tree_range;
			let index_range;

			if index_size + tree_size <= tree_data.range().size() {
				let mut range = tree_data.range();

				// shouldn't fail now
				tree_range = range.take_layout(Layout::from_size_align(tree_size, size_of::<usize>()).unwrap()).unwrap();
				index_range = range.take_layout(Layout::from_size_align(index_size, size_of::<usize>()).unwrap()).unwrap();

				// put this zone back into the metadata map
				if range.size() != 0 {
					metadata_zones.insert(range.size(), range).unwrap();
				}
			} else {
				if let Some(index_data) = MetaRange::new(index_size, &mut zones, &mut metadata_zones) {
					let mut orig_tree_range = tree_data.range();
					let mut orig_index_range = index_data.range();

					// shouldn't fail now
					tree_range = orig_tree_range.take_layout(Layout::from_size_align(tree_size, size_of::<usize>()).unwrap()).unwrap();
					index_range = orig_index_range.take_layout(Layout::from_size_align(index_size, size_of::<usize>()).unwrap()).unwrap();

					// put this zone back into the metadata map
					if orig_tree_range.size() != 0 {
						metadata_zones.insert(orig_tree_range.size(), orig_tree_range).unwrap();
					}

					if orig_index_range.size() != 0 {
						metadata_zones.insert(orig_index_range.size(), orig_index_range).unwrap();
					}
				} else {
					// give up on using this zone, use it for metadata instead
					metadata_zones.insert(current_size, current_zone.as_unaligned()).unwrap();

					// restore old zones before moving on to next zone
					tree_data.insert_into(&mut zones, &mut metadata_zones);
					continue;
				};
			}

			// technically undefined behavior to make a slice of uninitilized AtomicU8s, but in practice it shouldn't matter
			// they are initilized to 0 later anyways
			let tree_slice = unsafe {
				slice::from_raw_parts_mut(tree_range.as_usize() as *mut AtomicU8, unaligned_tree_size)
			};

			let index_slice = unsafe {
				slice::from_raw_parts_mut(index_range.as_usize() as *mut AtomicUsize, index_size)
			};

			let allocator = unsafe {
				PmemAllocator::from(current_zone, tree_slice, index_slice, PAGE_SIZE)
			};

			allocator_slice[i].write(allocator);

			i += 1;
		}

		let allocator_slice = unsafe {
			slice::from_raw_parts_mut(allocator_range.as_usize() as *mut PmemAllocator, i)
		};

		allocator_slice.sort_unstable_by(|a, b| a.start_addr().cmp(&b.start_addr()));

		PmemManager {
			allocers: allocator_slice,
			next_index: AtomicUsize::new(0),
		}
	}

	// gets index in search dealloc, where the zindex is not set
	fn get_index_of_allocation(&self, allocation: Allocation) -> Result<usize, usize> {
		self.allocers.binary_search_by(|allocer| allocer.start_addr().cmp(&allocation.as_usize()))
	}
}

// TODO: add realloc
impl PageAllocator for PmemManager {
	fn alloc(&self, layout: PageLayout) -> Option<Allocation> {
		assert!(layout.align() <= align_of(layout.size()), "PmemManager does not support allocations with a greater alignamant than size");

		// start allocating from different allocators to avoid slowing down each allocator with to many concurrent allocations
		let start_index = self.next_index.fetch_add(1, Ordering::Relaxed);

		for i in start_index..(start_index + self.allocers.len()) {
			let i = i % self.allocers.len();
			if let Some(mut allocation) = self.allocers[i].alloc(layout.size()) {
				allocation.zindex = i;
				return Some(allocation);
			}
		}

		None
	}

	unsafe fn dealloc(&self, allocation: Allocation) {
		// this will panic if allocation is not contained in the allocator
		unsafe {
			self.allocers[allocation.zindex].dealloc(allocation);
		}
	}

	unsafe fn search_dealloc(&self, allocation: Allocation) {
		match self.get_index_of_allocation(allocation) {
			Ok(index) => unsafe { self.allocers[index].dealloc(allocation) },
			Err(_) => panic!("could not find allocator that matched allocation"),
		}
	}
}
