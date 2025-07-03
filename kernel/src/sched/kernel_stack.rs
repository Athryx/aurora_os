use crate::{prelude::*, mem::{PageAllocation, PageLayout, PaRef}};

/// A kernel stack for a thread
#[derive(Debug)]
pub enum KernelStack {
    /// `KernelStack` will usually be the owned variant
    Owned(PageAllocation, PaRef),
    /// `Existing` is used just for the idle threads, when the stack was created before the thread structure
    Existing(AVirtRange),
}

impl KernelStack {
    pub const DEFAULT_SIZE: usize = PAGE_SIZE * 16;

    pub fn new(mut page_allocator: PaRef) -> KResult<Self> {
        let allocation = page_allocator
            .alloc(PageLayout::from_size_align(Self::DEFAULT_SIZE, PAGE_SIZE).unwrap())
            .ok_or(SysErr::OutOfMem)?;
        
        Ok(KernelStack::Owned(allocation, page_allocator))
    }

    pub fn as_virt_range(&self) -> AVirtRange {
        match self {
            Self::Owned(allocation, _) => allocation.as_vrange().try_as_aligned().unwrap(),
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
        if let Self::Owned(allocation, allocator) = self {
            unsafe { allocator.dealloc(*allocation); }
        }
    }
}