use core::cmp::min;

use crate::prelude::*;
use crate::alloc::{PaRef, HeapRef};
use crate::mem::{Allocation, PageLayout};
use crate::sync::{IMutex, IMutexGuard};
use super::{CapObject, CapType};

#[derive(Debug, Clone)]
struct AllocationEntry {
    allocation: Allocation,
    /// Offset from the start of memory capabity of this entry in bytes
    offset: usize,
}

/// An iterator over the virtual memory mappings that need to be made to map a given section of a memory cpaability
#[derive(Debug, Clone)]
pub struct MappedRegionsIterator<'a> {
    base_addr: VirtAddr,
    start_range_offset: usize,
    end_range_offset: usize,
    allocations: &'a [AllocationEntry],

    index: usize,
    current_offset: usize,
}

impl MappedRegionsIterator<'_> {
    pub fn without_unaligned_start(mut self) -> Self {
        if self.start_range_offset == 0 && self.index == 0 {
            self.start_range_offset = 0;
            self.index = 1;
        }
        self
    }

    pub fn without_unaligned_end(mut self) -> Self {
        self.end_range_offset = 0;
        self
    }

    /// Gets the virt range mapping for the entire first range, regardless of start offset
    pub fn get_entire_first_maping_range(&self) -> AVirtRange {
        let allocation = self.allocations[0].allocation;

        AVirtRange::new(self.base_addr - self.start_range_offset, allocation.size())
    }

    /// Gets the setion of the first mapping which is excluded by the start offset
    pub fn get_first_mapping_exluded_range(&self) -> AVirtRange {
        AVirtRange::new(self.base_addr - self.start_range_offset, self.start_range_offset)
    }
}

impl Iterator for MappedRegionsIterator<'_> {
    type Item = (AVirtRange, PhysAddr);

    fn next(&mut self) -> Option<Self::Item> {
        if self.index >= self.allocations.len() {
            return None;
        }

        let allocation = self.allocations[self.index].allocation;

        let (virt_range, phys_addr) = if self.allocations.len() == 1 {
            // there is only 1 entry, we need to consider both start and end offset
            let range_addr = self.base_addr;
            let range_size = allocation.size() - self.start_range_offset - self.end_range_offset;
            let virt_range = AVirtRange::new(range_addr, range_size);

            let phys_addr = allocation.addr().to_phys() + self.start_range_offset;

            (virt_range, phys_addr)
        } else if self.index == 0 {
            let virt_range = AVirtRange::new(self.base_addr, allocation.size() - self.start_range_offset);
            let phys_addr = allocation.addr().to_phys() + self.start_range_offset;

            (virt_range, phys_addr)
        } else if self.index == self.allocations.len() - 1 {
            let virt_range = AVirtRange::new(self.base_addr + self.current_offset, allocation.size() - self.end_range_offset);

            (virt_range, allocation.addr().to_phys())
        } else {
            (
                AVirtRange::new(self.base_addr + self.current_offset, allocation.size()),
                allocation.addr().to_phys(),
            )
        };

        self.index += 1;
        self.current_offset += virt_range.size();

        Some((virt_range, phys_addr))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let size = self.allocations.len() - self.index;

        (size, Some(size))
    }
}

#[derive(Debug)]
pub struct MemoryInner {
    allocations: Vec<AllocationEntry>,
    /// Total size of all allocations
    size: usize,
    page_allocator: PaRef,
    /// This is the number of locations the memory capability is currently mapped in
    pub map_ref_count: usize,
}

impl MemoryInner {
    /// Returns the size in bytes of this memory
    pub fn size(&self) -> usize {
        self.size
    }

    pub fn size_pages(&self) -> usize {
        self.size / PAGE_SIZE
    }

    /// Finds the index into the allocations array that the given offset from the start of the memory capability would be contained in
    fn allocation_index_of_offset(&self, offset: usize) -> Option<usize> {
        if offset >= self.size {
            return None;
        }

        match self.allocations.binary_search_by_key(&offset, |entry| entry.offset) {
            Ok(i) => Some(i),
            Err(i) => {
                // i cannot be 0 because the first allocation entry has offset 0
                Some(i - 1)
            }
        }
    }

    /// Iterates over the regions that would need to be mapped for a virtual mapping at `base_addr` of size `mapping_page_size`
    pub fn iter_mapped_regions(
        &self,
        base_addr: VirtAddr,
        mapping_offset_page: usize,
        mapping_page_size: usize,
    ) -> MappedRegionsIterator {
        assert!(mapping_offset_page + mapping_offset_page <= self.size_pages());

        let start_offset = mapping_offset_page * PAGE_SIZE;
        let end_offset = start_offset + mapping_page_size * PAGE_SIZE;

        let start_index = self.allocation_index_of_offset(start_offset).unwrap();
        let end_index = self.allocation_index_of_offset(end_offset - 1).unwrap();

        let start_allocation_entry = &self.allocations[start_index];
        let end_allocation_entry = &self.allocations[end_index];

        let start_range_offset = start_offset - start_allocation_entry.offset;
        let end_range_offset = end_allocation_entry.offset + end_allocation_entry.allocation.size() - end_offset;

        MappedRegionsIterator {
            base_addr,
            start_range_offset,
            end_range_offset,
            allocations: &self.allocations[start_index..=end_index],
            index: 0,
            current_offset: 0,
        }
    }

    /// Resizes the memory to be `new_page_size` by reallocating the last allocation
    /// 
    /// # Safety
    /// 
    /// Must check that memory is not mapped anywhere, because physical addressess of currently in use pages may be changed
    pub unsafe fn resize_end(&mut self, new_page_size: usize) -> KResult<()> {
        if new_page_size == 0 {
            return Err(SysErr::InvlArgs);
        }

        let layout = PageLayout::from_size_align(new_page_size * PAGE_SIZE, PAGE_SIZE)
            .expect("failed to make page layout");
        
        let end_index = self.allocations.len() - 1;

        self.allocations[end_index].allocation = unsafe {
            self.page_allocator.realloc(self.allocations[end_index].allocation, layout)
                .ok_or(SysErr::OutOfMem)?
        };

        Ok(())
    }
}

/// A capability that represents memory that can be mapped into a process
#[derive(Debug)]
pub struct Memory {
    inner: IMutex<MemoryInner>,
}

impl Memory {
    /// Returns an error is pages is size 0
    pub fn new(mut page_allocator: PaRef, heap_allocator: HeapRef, pages: usize) -> KResult<Self> {
        if pages == 0 {
            return Err(SysErr::InvlArgs);
        }

        let first_allocation = page_allocator.alloc(
            PageLayout::from_size_align(pages * PAGE_SIZE, PAGE_SIZE)
                .expect("could not create page layout for Memory capability"),
        ).ok_or(SysErr::OutOfMem)?;

        let mut allocations = Vec::new(heap_allocator);
        allocations.push(AllocationEntry {
            allocation: first_allocation,
            offset: 0,
        })?;

        let inner = MemoryInner {
            allocations,
            size: first_allocation.size(),
            page_allocator,
            map_ref_count: 0,
        };

        Ok(Memory { inner: IMutex::new(inner) })
    }

    pub fn inner(&self) -> IMutexGuard<MemoryInner> {
        self.inner.lock()
    }
}

impl CapObject for Memory {
    const TYPE: CapType = CapType::Memory;
}