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
use super::{HeapAllocator, AllocRef, PaRef, PageAllocator};
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
    /// Creates a new metadate range
    /// 
    /// The zone is taken from the meta map, or the main map if no zone in the meta map is large enough
    fn new(size: usize, main_map: &mut MainMap, meta_map: &mut MetaMap) -> Option<Self> {
        match meta_map.remove_zone_at_least_size(size) {
            Some(range) => Some(MetaRange::Meta(range)),
            None => main_map.remove_zone_at_least_size(size).map(MetaRange::Main),
        }
    }

    /// Puts this metadata ranges's zone back where it came from (either main map or mata map)
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

/// Iterates over all the sections of size aligned pages in an AVirtRange
// TODO: maybe put this as a method on AVirtRange if it is ever used anywhere else
#[derive(Clone)]
struct SizeAlignedIter {
    start: usize,
    end: usize,
}

impl SizeAlignedIter {
    fn new(range: AVirtRange) -> Self {
        SizeAlignedIter {
            start: range.as_usize(),
            end: range.end_usize(),
        }
    }
}

impl Iterator for SizeAlignedIter {
    type Item = AVirtRange;

    fn next(&mut self) -> Option<Self::Item> {
        if self.start >= self.end {
            return None;
        }

        let size = min(align_of(self.start), 1 << log2(self.end - self.start));

        let out = AVirtRange::new(VirtAddr::new(self.start), size);

        self.start += size;

        Some(out)
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
        // iterator that finds all the usable memory regions,
        // and splits them up into size aligned chunks
        // (so they are valid to be use for PmemAllocator)
        let zone_iter = mem_map
            .iter()
            .filter(|zone| matches!(zone, MemoryRegionType::Usable(_)))
            .filter_map(|mem| mem.range().to_virt().as_inside_aligned())
            .flat_map(SizeAlignedIter::new);

        // biggest usable zone, used for bootstrap heap
        let init_heap_vrange = zone_iter
            .clone()
            .reduce(|z1, z2| if z1.size() > z2.size() { z1 } else { z2 })
            .expect("no usable memory zones found");

        // A fixed page allocator used as the initial page allocator
        // panic safety: this range is the biggest range, it should not fail
        let page_allocator = unsafe { FixedPageAllocator::new(init_heap_vrange) };
        let pa_ptr = &page_allocator as *const dyn PageAllocator;
        let page_ref = unsafe { PaRef::new_raw(pa_ptr) };

        let allocer = LinkedListAllocator::new(page_ref);
        let temp = &allocer as *const dyn HeapAllocator;
        // Safety: make sure not to use this outside of this function
        let aref = unsafe { AllocRef::new_raw(temp) };

        // holds zones of memory that have a size of power of 2 and an alignmant equal to their size
        let mut zones = ZoneMap::new(aref.clone());

        // holds zones taken from zones vecmap that are used to store metadata
        let mut metadata_zones = ZoneMap::new(aref);

        for range in zone_iter {
            zones.insert(range)
                .expect("not enough memory to build zone map for pmem manager");
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
        let allocator_range = orig_allocator_range
            .take_layout(Layout::array::<PmemAllocator>(allocator_count).unwrap())
            .unwrap();

        // store the other part in the metadata array
        if orig_allocator_range.size() != 0 {
            metadata_zones.insert(orig_allocator_range).unwrap();
        }

        let allocator_slice = unsafe {
            slice::from_raw_parts_mut(
                allocator_range.as_usize() as *mut MaybeUninit<PmemAllocator>,
                allocator_count,
            )
        };

        // index of current allocator
        let mut i = 0;

        // total amount of allocatable memory
        let mut total_mem_size = 0;

        while let Some(current_zone) = zones.remove_largest_zone() {
            let (unaligned_tree_size, index_size) =
                PmemAllocator::required_tree_index_size(current_zone, PAGE_SIZE).unwrap();
            let tree_size = align_up(unaligned_tree_size, size_of::<usize>());

            let tree_data = if let Some(data) = MetaRange::new(tree_size, &mut zones, &mut metadata_zones) {
                data
            } else {
                // give up on using this zone, use it for metadata instead,
                // but only if it is not being used by the bootstrap heap, otherwise discard
                if !init_heap_vrange.contains_range(&current_zone) {
                    metadata_zones.insert(current_zone.as_unaligned()).unwrap();
                }
                continue;
            };

            let tree_range;
            let index_range;

            if index_size + tree_size <= tree_data.range().size() {
                let mut range = tree_data.range();

                // shouldn't fail now
                tree_range = range
                    .take_layout(Layout::from_size_align(tree_size, size_of::<usize>()).unwrap())
                    .unwrap();
                index_range = range
                    .take_layout(Layout::from_size_align(index_size, size_of::<usize>()).unwrap())
                    .unwrap();

                // put this zone back into the metadata map
                if range.size() != 0 {
                    metadata_zones.insert(range).unwrap();
                }
            } else if let Some(index_data) = MetaRange::new(index_size, &mut zones, &mut metadata_zones) {
                let mut orig_tree_range = tree_data.range();
                let mut orig_index_range = index_data.range();

                // shouldn't fail now
                tree_range = orig_tree_range
                    .take_layout(Layout::from_size_align(tree_size, size_of::<usize>()).unwrap())
                    .unwrap();
                index_range = orig_index_range
                    .take_layout(Layout::from_size_align(index_size, size_of::<usize>()).unwrap())
                    .unwrap();

                // put this zone back into the metadata map
                if orig_tree_range.size() != 0 {
                    metadata_zones.insert(orig_tree_range).unwrap();
                }

                if orig_index_range.size() != 0 {
                    metadata_zones.insert(orig_index_range).unwrap();
                }
            } else {
                // restore old zones before moving on to next zone
                tree_data.insert_into(&mut zones, &mut metadata_zones);

                // give up on using this zone, use it for metadata instead,
                // but only if it is not being used by the bootstrap heap, otherwise discard
                if !init_heap_vrange.contains_range(&current_zone) {
                    metadata_zones.insert(current_zone.as_unaligned()).unwrap();
                }
                continue;
            }

            // technically undefined behavior to make a slice of uninitilized AtomicU8s, but in practice it shouldn't matter
            // they are initilized to 0 later anyways
            let tree_slice = unsafe {
                slice::from_raw_parts_mut(tree_range.as_usize() as *mut AtomicU8, unaligned_tree_size)
            };

            let index_slice =
                unsafe { slice::from_raw_parts_mut(index_range.as_usize() as *mut AtomicUsize, index_size) };

            let allocator = unsafe { PmemAllocator::from(current_zone, tree_slice, index_slice, PAGE_SIZE) };

            total_mem_size += current_zone.page_size();

            allocator_slice[i].write(allocator);

            i += 1;
        }

        let allocator_slice =
            unsafe { slice::from_raw_parts_mut(allocator_slice.as_mut_ptr() as *mut PmemAllocator, i) };

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
        self.allocers
            .binary_search_by(|allocer| allocer.start_addr().cmp(&allocation.as_usize()))
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
            // if index is 0, there is no allocator that contains this allocation
            // because there has to be an allocator with a start address befor 0
            Err(index) if index != 0 => unsafe { self.allocers[index - 1].dealloc(allocation) },
            _ => panic!("could not find allocator that matched allocation"),
        }
    }
}
