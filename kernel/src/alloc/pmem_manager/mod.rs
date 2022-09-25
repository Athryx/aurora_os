mod pmem_allocator;
mod zone_map;

use core::alloc::Layout;
use core::cmp::min;
use core::mem::MaybeUninit;
use core::slice;
use core::sync::atomic::{AtomicU8, AtomicUsize, Ordering};

use pmem_allocator::PmemAllocator;
use zone_map::ZoneMap;

use super::fixed_page_allocator::FixedPageAllocator;
use super::linked_list_allocator::LinkedListAllocator;
use super::{OrigAllocator, OrigRef, PaRef, PageAllocator};
use crate::consts::{AP_CODE_END, AP_CODE_RANGE, AP_CODE_START};
use crate::mb2::{MemoryMap, MemoryRegionType};
use crate::mem::{Allocation, PageLayout};
use crate::prelude::*;

// metadata range
enum MetaRange {
    Main(AVirtRange),
    Meta(UVirtRange),
}

type MainMap = ZoneMap<AVirtRange>;
type MetaMap = ZoneMap<UVirtRange>;

impl MetaRange {
    fn new(size: usize, main_map: &mut MainMap, meta_map: &mut MetaMap) -> Option<Self> {
        match meta_map.remove_zone_at_least_size(size) {
            Some(range) => Some(MetaRange::Meta(range)),
            None => match main_map.remove_zone_at_least_size(size) {
                Some(range) => Some(MetaRange::Main(range)),
                None => None,
            },
        }
    }

    fn insert_into(&self, main_map: &mut MainMap, meta_map: &mut MetaMap) {
        match *self {
            Self::Main(range) => {
                main_map.insert(range).unwrap();
            },
            Self::Meta(range) => {
                meta_map.insert(range).unwrap();
            },
        }
    }

    fn range(&self) -> UVirtRange {
        match self {
            Self::Main(range) => range.as_unaligned(),
            Self::Meta(range) => *range,
        }
    }
}

pub struct PmemManager {
    allocers: &'static [PmemAllocator],
    next_index: AtomicUsize,
}

impl PmemManager {
    // TODO: this might encounter problems with low amount of system memory (like very low)
    /// Creates a new PmemManager from the memory map
    /// Also returns the total amount of bytes that can be allocated, used to set up the root allocator
    pub unsafe fn new(mem_map: &MemoryMap) -> (PmemManager, usize) {
        // iterator over usable memory zones as a VirtRange
        let usable = mem_map
            .iter()
            .filter(|zone| matches!(zone, MemoryRegionType::Usable(_)))
            .filter_map(|mem| mem.range().to_virt().as_inside_aligned())
            .flat_map(|mem| {
                // FIXME: this is really ugly code
                // filters out ap code zone from usable memory range
                if mem.contains_range(&*AP_CODE_RANGE) {
                    let (start, end) = mem.split_at(*AP_CODE_RANGE);
                    [start, end].into_iter()
                } else {
                    [Some(mem), None].into_iter()
                }
                .filter_map(|elem| elem)
            });

        // biggest usable virt range
        // align to pages because we will use this for the initial allocator
        let max = usable.clone().reduce(|z1, z2| if z1.size() > z2.size() { z1 } else { z2 }).expect("no usable memory zones found");

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
        let page_ref = unsafe { PaRef::new_raw(pa_ptr) };

        let allocer = LinkedListAllocator::new(page_ref);
        let temp = &allocer as *const dyn OrigAllocator;
        // Safety: make sure not to use this outside of this function
        let aref = unsafe { OrigRef::new_raw(temp) };

        // holds zones of memory that have a size of power of 2 and an alignmant equal to their size
        let mut zones = ZoneMap::new(aref.downgrade());

        // holds zones taken from zones vecmap that are used to store metadata
        let mut metadata_zones = ZoneMap::new(aref.downgrade());

        for region in usable {
            let mut start = region.as_usize();
            let end = region.end_usize();

            while start < end {
                let size = min(align_of(start), 1 << log2(end - start));
                // because region is aligned, this should be aligned
                let range = AVirtRange::new(VirtAddr::new(start), size);
                zones.insert(range).expect("not enough memory to build zone map for pmem manager");

                start += size;
            }
        }

        // one zone will be used to store the allocators
        let allocator_count = zones.len() - 1;

        // get slice of memory to hold PmemAllocators
        // not optimal prediction of how many allocators there will be, but there can't be more
        let size = allocator_count * size_of::<PmemAllocator>();

        // get a region of memory to store all of the allocators
        let orig_allocator_range = zones.remove_zone_at_least_size(size).unwrap();
        let mut orig_allocator_range = orig_allocator_range.as_unaligned();

        assert!(
            !init_heap_vrange.contains_range(&orig_allocator_range),
            "tried to use memory range for allocator initilizer heap to store allocator objects"
        );

        // only get part that is needed to store all allocator objects
        let allocator_range = orig_allocator_range.take_layout(Layout::array::<PmemAllocator>(allocator_count).unwrap()).unwrap();

        // store the other part in the metadata array
        if orig_allocator_range.size() != 0 {
            metadata_zones.insert(orig_allocator_range).unwrap();
        }

        let allocator_slice = unsafe { slice::from_raw_parts_mut(allocator_range.as_usize() as *mut MaybeUninit<PmemAllocator>, allocator_count) };

        // index of current allocator
        let mut i = 0;

        // total amount of allocatable memory
        let mut total_mem_size = 0;

        while let Some(current_zone) = zones.remove_largest_zone() {
            let (unaligned_tree_size, index_size) = PmemAllocator::required_tree_index_size(current_zone, PAGE_SIZE).unwrap();
            let tree_size = align_up(unaligned_tree_size, size_of::<usize>());

            let tree_data = if let Some(data) = MetaRange::new(tree_size, &mut zones, &mut metadata_zones) {
                data
            } else {
                // give up on using this zone, use it for metadata instead
                metadata_zones.insert(current_zone.as_unaligned()).unwrap();
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
                    metadata_zones.insert(range).unwrap();
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
                        metadata_zones.insert(orig_tree_range).unwrap();
                    }

                    if orig_index_range.size() != 0 {
                        metadata_zones.insert(orig_index_range).unwrap();
                    }
                } else {
                    // give up on using this zone, use it for metadata instead
                    metadata_zones.insert(current_zone.as_unaligned()).unwrap();

                    // restore old zones before moving on to next zone
                    tree_data.insert_into(&mut zones, &mut metadata_zones);
                    continue;
                };
            }

            // technically undefined behavior to make a slice of uninitilized AtomicU8s, but in practice it shouldn't matter
            // they are initilized to 0 later anyways
            let tree_slice = unsafe { slice::from_raw_parts_mut(tree_range.as_usize() as *mut AtomicU8, unaligned_tree_size) };

            let index_slice = unsafe { slice::from_raw_parts_mut(index_range.as_usize() as *mut AtomicUsize, index_size) };

            let allocator = unsafe { PmemAllocator::from(current_zone, tree_slice, index_slice, PAGE_SIZE) };

            total_mem_size += current_zone.page_size();

            allocator_slice[i].write(allocator);

            i += 1;
        }

        let allocator_slice = unsafe { slice::from_raw_parts_mut(allocator_range.as_usize() as *mut PmemAllocator, i) };

        allocator_slice.sort_unstable_by(|a, b| a.start_addr().cmp(&b.start_addr()));

        (
            PmemManager {
                allocers: allocator_slice,
                next_index: AtomicUsize::new(0),
            },
            total_mem_size,
        )
    }

    // gets index in search dealloc, where the zindex is not set
    fn get_index_of_allocation(&self, allocation: Allocation) -> Result<usize, usize> {
        self.allocers.binary_search_by(|allocer| allocer.start_addr().cmp(&allocation.as_usize()))
    }
}

// TODO: add realloc
impl PageAllocator for PmemManager {
    fn alloc(&self, layout: PageLayout) -> Option<Allocation> {
        assert!(
            layout.align() <= align_of(layout.size()),
            "PmemManager does not support allocations with a greater alignamant than size"
        );

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
