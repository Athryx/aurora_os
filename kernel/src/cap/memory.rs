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
    pub fn new(mut page_allocator: PaRef, pages: usize) -> KResult<Self> {
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
}

impl CapObject for Memory {
    const TYPE: CapType = CapType::Memory;
}