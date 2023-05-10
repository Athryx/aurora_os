use crate::prelude::*;
use crate::alloc::PaRef;
use crate::mem::{Allocation, PageLayout};
use crate::sync::{IMutex, IMutexGuard};
use super::{CapObject, CapType};

#[derive(Debug)]
pub struct MemoryInner {
    allocation: Allocation,
    page_allocator: PaRef,
    /// This is the number of locations the memory capability is currently mapped in
    pub map_ref_count: usize,
}

impl MemoryInner {
    pub fn allocation(&self) -> Allocation {
        self.allocation
    }

    pub fn phys_addr(&self) -> PhysAddr {
        self.allocation.addr().to_phys()
    }

    /// Returns the size in bytes of this memory
    pub fn size(&self) -> usize {
        self.allocation.size()
    }

    pub fn size_pages(&self) -> usize {
        self.allocation.size() / PAGE_SIZE
    }

    /// Resizes the allocation to be `new_page_size`
    /// 
    /// # Safety
    /// 
    /// Must check that memory is not mapped anywhere its not supposed to be
    pub unsafe fn resize(&mut self, new_page_size: usize) -> KResult<()> {
        if new_page_size == 0 {
            return Err(SysErr::InvlArgs);
        }

        let layout = PageLayout::from_size_align(new_page_size * PAGE_SIZE, PAGE_SIZE)
            .expect("failed to make page layout");
        
        self.allocation = unsafe {
            self.page_allocator.realloc(self.allocation, layout)
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
    pub fn new(mut page_allocator: PaRef, pages: usize) -> KResult<Self> {
        if pages == 0 {
            return Err(SysErr::InvlArgs);
        }

        let inner = MemoryInner {
            allocation: page_allocator
                .alloc(
                    PageLayout::from_size_align(pages * PAGE_SIZE, PAGE_SIZE)
                        .expect("could not create page layout for Memory capability"),
                ).ok_or(SysErr::OutOfMem)?,
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