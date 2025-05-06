use crate::{prelude::*, mem::{Allocation, PageLayout}, allocator::zm};

/// A kernel stack for a thread
#[derive(Debug)]
pub enum KernelStack {
    /// `KernelStack` will usually be the owned variant
    Owned(Allocation),
    /// `Existing` is used just for the idle threads, when the stack was created before the thread structure
    Existing(AVirtRange),
}

impl KernelStack {
    pub const DEFAULT_SIZE: usize = PAGE_SIZE * 16;

    pub fn new() -> KResult<Self> {
        let allocation = zm()
            .alloc(PageLayout::from_size_align(Self::DEFAULT_SIZE, PAGE_SIZE).unwrap())
            .ok_or(SysErr::OutOfMem)?;
        
        Ok(KernelStack::Owned(allocation))
    }

    pub fn as_virt_range(&self) -> AVirtRange {
        match self {
            Self::Owned(allocation) => allocation.as_vrange().try_as_aligned().unwrap(),
            Self::Existing(virt_range) => *virt_range,
        }
    }

    pub fn stack_base(&self) -> VirtAddr {
        self.as_virt_range().addr()
    }

    pub fn stack_top(&self) -> VirtAddr {
        self.as_virt_range().end_addr()
    }
}

impl Drop for KernelStack {
    fn drop(&mut self) {
        if let Self::Owned(allocation) = self {
            unsafe { zm().dealloc(*allocation); }
        }
    }
}