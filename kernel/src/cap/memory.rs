use crate::prelude::*;
use crate::alloc::PaRef;
use crate::mem::{Allocation, PageLayout};
use super::{CapObject, CapType};

/// A capability that represents memory that can be mapped into a process
#[derive(Debug)]
pub struct Memory {
    allocation: Allocation,
}

impl Memory {
    /// Returns an error is pages is size 0
    pub fn new(mut page_allocator: PaRef, pages: usize) -> KResult<Self> {
        if pages == 0 {
            return Err(SysErr::InvlArgs);
        }

        Ok(Memory {
            allocation: page_allocator
                .allocator()
                .alloc(
                    PageLayout::from_size_align(pages * PAGE_SIZE, PAGE_SIZE)
                        .expect("could not create page layout for Memory capability"),
                ).ok_or(SysErr::OutOfMem)?,
        })
    }

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
}

impl CapObject for Memory {
    const TYPE: CapType = CapType::Memory;
}