use core::cmp::min;

use crate::prelude::*;
use crate::alloc::{PaRef, HeapRef};
use crate::mem::{Allocation, PageLayout};
use crate::sync::{IrwLock, IrwLockReadGuard, IrwLockWriteGuard};
use super::{CapObject, CapType};

#[derive(Debug, Clone, Copy)]
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
    // TODO: maybe make this an atomic usize, so this can be incramented and decramented without needing write access in memory map and unmap
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

    /// Shrinks this memory capability to be the given size
    /// 
    /// # Panics
    /// 
    /// panics if new_page_size is greater than the size of the memory capability, or new_page_size is 0
    /// 
    /// # Safety
    /// 
    /// the parts that are being shrunk must not be mapped in memory
    unsafe fn shrink_memory(&mut self, new_page_size: usize) {
        assert!(new_page_size <= self.size_pages());
        assert!(new_page_size != 0);

        let mut shrink_amount = (self.size_pages() - new_page_size) * PAGE_SIZE;

        for i in (0..self.allocations.len()).rev() {
            let allocation = self.allocations[i].allocation;

            if allocation.size() >= shrink_amount {
                self.allocations.pop();
                shrink_amount -= allocation.size();
                self.size -= allocation.size();

                unsafe {
                    self.page_allocator.dealloc(allocation);
                }
            } else {
                let new_allocation_size = allocation.size() - shrink_amount;

                // panic safety; realloc in place should never fail
                let new_allocation = unsafe {
                    self.page_allocator.realloc_in_place(allocation, PageLayout::new_rounded(new_allocation_size, PAGE_SIZE).unwrap())
                }.unwrap();

                self.allocations[i].allocation = new_allocation;

                let size_change = allocation.size() - new_allocation.size();
                self.size -= size_change;

                break;
            }

            if shrink_amount == 0 {
                break;
            }
        }
    }

    /// Attempts to grow the memory in this capability without moving any already allocated memory
    fn grow_in_place(&mut self, new_page_size: usize) -> KResult<()> {
        assert!(new_page_size >= self.size_pages());

        let grow_amount = (new_page_size - self.size_pages()) * PAGE_SIZE;

        // panic safety: there should always be at least 1 allocation
        let last_entry = self.allocations.last().unwrap();
        let last_allocation = last_entry.allocation;

        // TODO: maybe add support to realloc in place as large as possibel so even in event of realloc
        // failure, the last allocation would be grown by at least something
        let result = unsafe {
            self.page_allocator.realloc_in_place(
                last_allocation,
                PageLayout::new_rounded(last_allocation.size() + grow_amount, PAGE_SIZE).unwrap(),
            )
        };

        if let Some(new_allocation) = result {
            let size_change = new_allocation.size() - last_allocation.size();
            self.size += size_change;

            self.allocations.last_mut().unwrap().allocation = new_allocation;

            Ok(())
        } else {
            // add new allocation because reallocating end failed
            let Some(new_allocation) = self.page_allocator
                .alloc(PageLayout::new_rounded(grow_amount, PAGE_SIZE).unwrap()) else {
                return Err(SysErr::OutOfMem);
            };

            if let Err(error) = self.allocations.push(AllocationEntry {
                allocation: new_allocation,
                offset: last_entry.offset + last_allocation.size(),
            }) {
                // could not append allocation entry, deallocate new allocation
                // safety: the new allocation is not used anywhere
                unsafe {
                    self.page_allocator.dealloc(new_allocation);
                }

                Err(error)
            } else {
                self.size += new_allocation.size();

                Ok(())
            }   
        }
    }

    /// Grows the memory on this capability, but does not necesarily grow it in place, so it is unsafe to have this memory mapped if grow is called
    unsafe fn grow(&mut self, new_page_size: usize) -> KResult<()> {
        assert!(new_page_size >= self.size_pages());

        let grow_amount = (new_page_size - self.size_pages()) * PAGE_SIZE;

        // panic safety: there should always be at least 1 allocation
        let last_allocation = self.allocations.last().unwrap().allocation;

        let new_allocation = unsafe {
            self.page_allocator.realloc(
                last_allocation,
                PageLayout::new_rounded(last_allocation.size() + grow_amount, PAGE_SIZE).unwrap(),
            ).ok_or(SysErr::OutOfMem)?
        };

        self.allocations.last_mut().unwrap().allocation = new_allocation;
        
        let size_change = new_allocation.size() - last_allocation.size();
        self.size += size_change;

        Ok(())
    }

    /// Resizes the memory to the given size in pages
    /// 
    /// The in place version of resize ensures that all pointers to memory areas inside this memory
    /// that point to an offset less than the new size will remain valid
    /// 
    /// # Safety
    /// 
    /// This memory capability must not have any part past the end pf the new size mapped in memory
    pub unsafe fn resize_in_place(&mut self, new_page_size: usize) -> KResult<()> {
        if new_page_size > self.size_pages() {
            self.grow_in_place(new_page_size)
        } else {
            unsafe {
                self.shrink_memory(new_page_size)
            }

            Ok(())
        }
    }

    /// Resizes the memory to the given size in pages
    /// 
    /// The out of place version of resize does not ensure that all pointers to memory areas inside this memory
    /// that point to an offset less than the new size will remain valid
    /// 
    /// # Safety
    /// 
    /// This memory capability must not be mapped
    pub unsafe fn resize_out_of_place(&mut self, new_page_size: usize) -> KResult<()> {
        if new_page_size > self.size_pages() {
            unsafe {
                self.grow(new_page_size)
            }
        } else {
            unsafe {
                self.shrink_memory(new_page_size)
            }

            Ok(())
        }
    }

    /// Writes the data at the given offset in the memory capability
    /// 
    /// If the write is out of bounds of this memory capability, only the in bounds part is written
    /// 
    /// # Returns
    /// 
    /// the number of bytes written
    /// 
    /// # Safety
    /// 
    /// Must not write to any memory used by anything else, or a place that userspace doesn't expect
    pub unsafe fn write(&self, mut data: &[u8], offset: usize) -> usize {
        let Some(mut index) = self.allocation_index_of_offset(offset) else {
            return 0;
        };

        let mut allocation_offset = offset - self.allocations[index].offset;
        let mut total_write_size = 0;

        loop {
            let mut allocation = self.allocations[index].allocation;

            let write_size = min(data.len(), allocation.size() - allocation_offset);

            // safety: caller must ensure that this memory capability only stores userspace data expecting to be written to
            unsafe {
                allocation.copy_from_mem_offset(&data[..write_size], allocation_offset);
            }

            total_write_size += write_size;
            data = &data[write_size..];

            allocation_offset = 0;
            index += 1;

            if data.len() == 0 || index >= self.allocations.len() {
                break;
            }
        }

        total_write_size
    }

    /// Zeros this entire memory capability
    /// 
    /// # Safety
    /// 
    /// Must not write to any memory used by anything else, or a place that userspace doesn't expect
    pub unsafe fn zero(&self) {
        for allocation in self.allocations.iter() {
            let mut raw_allocation = allocation.allocation;
            let allocation_slice = raw_allocation.as_mut_slice_ptr();

            unsafe {
                // TODO: figure out if this might need to be volatile
                // safety: caller must ensure that this memory capability only stores userspace data expecting to be written to
                ptr::write_bytes(allocation_slice.as_mut_ptr(), 0, allocation_slice.len());
            }
        }
    }
}

/// A capability that represents memory that can be mapped into a process
#[derive(Debug)]
pub struct Memory {
    inner: IrwLock<MemoryInner>,
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

        Ok(Memory { inner: IrwLock::new(inner) })
    }

    pub fn inner_read(&self) -> IrwLockReadGuard<MemoryInner> {
        self.inner.read()
    }

    pub fn inner_write(&self) -> IrwLockWriteGuard<MemoryInner> {
        self.inner.write()
    }
}

impl CapObject for Memory {
    const TYPE: CapType = CapType::Memory;
}