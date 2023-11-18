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
use super::{HeapRef, PaRef, PageAllocator};
use crate::mb2::{MemoryMap, MemoryRegionType};
use crate::mem::{Allocation, PageLayout};
use crate::prelude::*;

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
    pub(super) allocers: &'static [PmemAllocator],
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
        let pa_ptr = &page_allocator as *const FixedPageAllocator;
        let page_ref = unsafe { PaRef::init_allocator(pa_ptr) };

        let init_heap_allocator = LinkedListAllocator::new(page_ref);
        let init_allocator_ptr = &init_heap_allocator as *const LinkedListAllocator;
        // Safety: make sure not to use this outside of this function
        let aref = unsafe { HeapRef::init_allocator(init_allocator_ptr) };

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
            let unaligned_tree_size = PmemAllocator::required_tree_array_size(current_zone, PAGE_SIZE).unwrap();
            let tree_size = align_up(unaligned_tree_size, size_of::<usize>());

            let tree_zone = match metadata_zones.remove_zone_at_least_size(tree_size) {
                Some(range) => Some(range),
                None => zones.remove_zone_at_least_size(tree_size).map(|range| range.as_unaligned()),
            };

            let Some(mut tree_zone) = tree_zone else {
                // give up on using this zone, use it for metadata instead,
                // but only if it is not being used by the bootstrap heap, otherwise discard
                if !init_heap_vrange.contains_range(&current_zone) {
                    metadata_zones.insert(current_zone.as_unaligned()).unwrap();
                }
                continue;
            };

            let tree_range = tree_zone
                .take_layout(Layout::from_size_align(tree_size, size_of::<usize>()).unwrap())
                .unwrap();

            // put tree data range back into metadata slice if it is not yet depleted
            if tree_zone.size() != 0 {
                metadata_zones.insert(tree_zone).unwrap();
            }

            // technically undefined behavior to make a slice of uninitilized AtomicU8s, but in practice it shouldn't matter
            // they are initilized to 0 later anyways
            let tree_slice = unsafe {
                slice::from_raw_parts_mut(tree_range.as_usize() as *mut AtomicU8, unaligned_tree_size)
            };

            let allocator = unsafe { PmemAllocator::from(current_zone, tree_slice, PAGE_SIZE) };

            total_mem_size += current_zone.page_size();

            allocator_slice[i].write(allocator);

            i += 1;
        }

        let allocator_slice =
            unsafe { slice::from_raw_parts_mut(allocator_slice.as_mut_ptr() as *mut PmemAllocator, i) };

        allocator_slice.sort_unstable_by_key(|a| a.start_addr());

        (
            PmemManager {
                allocers: allocator_slice,
                next_index: AtomicUsize::new(0),
            },
            total_mem_size,
        )
    }

    /// Returns the size that would be allocated for the given page layout
    pub fn get_allocation_size_for_layout(layout: PageLayout) -> usize {
        1 << log2_up(layout.size())
    }

    // gets index in search dealloc, where the zindex is not set
    fn get_allocator_for_allocation(&self, allocation: Allocation) -> &PmemAllocator {
        if let Some(index) = allocation.zindex {
            &self.allocers[index]
        } else {
            let result = self.allocers
                .binary_search_by(|allocer| allocer.start_addr().cmp(&allocation.as_usize()));

            match result {
                Ok(index) => &self.allocers[index],
                // if index is 0, there is no allocator that contains this allocation
                // because there has to be an allocator with a start address befor 0
                Err(index) if index != 0 => &self.allocers[index - 1],
                _ => panic!("could not find allocator that matched allocation"),
            }
        }
    }

    /// Takes in allocator that allocation was allocated from and performs reallocation
    /// 
    /// Called by both realloc and search_realloc
    unsafe fn realloc_inner(&self, allocator: &PmemAllocator, allocation: Allocation, layout: PageLayout) -> Option<Allocation> {
        assert!(
            layout.align() <= align_of(layout.size()),
            "PmemManager does not support allocations with a greater alignamant than size"
        );

        if let Some(new_allocation) = unsafe { allocator.realloc_in_place(allocation, layout.size()) } {
            Some(new_allocation)
        } else {
            let mut out = self.alloc(layout)?;
            unsafe {
                // safety: allocations do not overlap because alloc will ensure they don't overlap
                out.copy_from_mem(allocation.as_slice_ptr());
                allocator.dealloc(allocation);
            }
            Some(out)
        }
    }
}

unsafe impl PageAllocator for PmemManager {
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
                allocation.zindex = Some(i);
                return Some(allocation);
            }
        }

        None
    }

    unsafe fn dealloc(&self, allocation: Allocation) {
        // this will panic if allocation is not contained in the allocator
        unsafe {
            self.get_allocator_for_allocation(allocation).dealloc(allocation);
        }
    }

    unsafe fn realloc(&self, allocation: Allocation, layout: PageLayout) -> Option<Allocation> {
        unsafe {
            self.realloc_inner(self.get_allocator_for_allocation(allocation), allocation, layout)
        }
    }


    unsafe fn realloc_in_place(&self, allocation: Allocation, layout: PageLayout) -> Option<Allocation> {
        assert!(
            layout.align() <= align_of(layout.size()),
            "PmemManager does not support allocations with a greater alignamant than size"
        );

        unsafe {
            self.get_allocator_for_allocation(allocation).realloc_in_place(allocation, layout.size())
        }
    }
}
