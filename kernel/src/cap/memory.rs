use core::cmp::min;

use crate::prelude::*;
use crate::alloc::{PaRef, HeapRef};
use crate::mem::{Allocation, PageLayout};
use crate::sync::{IMutex, IMutexGuard};
use super::{CapObject, CapType};

#[derive(Debug)]
pub struct MemoryInner {
    allocations: Vec<Allocation>,
    /// Size of thie memory capability that will be mapped into memory
    /// 
    /// This is less than `total_size` in some scenerios
    size: usize,
    /// Total size of all allocations
    total_size: usize,
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

    pub fn iter_mapped_regions<'a>(&'a self, mut base_addr: VirtAddr) -> impl Iterator<Item = (AVirtRange, PhysAddr)> + Clone + 'a {
        let mut remaining_size = self.size;

        self.allocations.iter()
            .map(move |allocation| {
                let virt_range = AVirtRange::new(base_addr, min(remaining_size, allocation.size()));

                base_addr += virt_range.size();
                remaining_size -= virt_range.size();

                (virt_range, allocation.addr().to_phys())
            })
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

        self.allocations[end_index] = unsafe {
            self.page_allocator.realloc(self.allocations[end_index], layout)
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
        allocations.push(first_allocation)?;

        let inner = MemoryInner {
            allocations,
            size: first_allocation.size(),
            total_size: first_allocation.size(),
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