use core::fmt::{self, Debug};

use crate::mem::{Allocation, PageLayout};
use crate::container::Arc;
use crate::prelude::*;

use super::cap_allocator::CapAllocatorWrapper;
use super::fixed_page_allocator::FixedPageAllocator;
use super::pmem_manager::PmemManager;
use super::{zm, CapAllocator};

/// A trait that represents an object that can allocate physical memory pages
// NOTE: this isn't really necessary anymore now that PaRef is &mut self with these, just exists for documentation purposes
pub unsafe trait PageAllocator: Send + Sync {
    /// Allocates a page according to page layout
    fn alloc(&self, layout: PageLayout) -> Option<Allocation>;

    /// Deallocate pages, uses the zindex field as metadata to deallocate the allocation if it is not None
    unsafe fn dealloc(&self, allocation: Allocation);

    /// Reallocates the allocation to match the layout
    unsafe fn realloc(&self, allocation: Allocation, layout: PageLayout) -> Option<Allocation> {
        let mut out = self.alloc(layout)?;
        unsafe {
            // safety: allocations do not overlap because alloc will ensure they don't overlap
            out.copy_from_mem(allocation.as_slice_ptr());
            self.dealloc(allocation);
        }
        Some(out)
    }

    unsafe fn realloc_in_place(&self, _allocation: Allocation, _layout: PageLayout) -> Option<Allocation> {
        None
    }
}

// this is in inner enum so InitAllocator cannot be constructed without unsafe
#[derive(Clone)]
enum PaRefInner {
    PmemManager(&'static PmemManager),
    InitAllocator(*const FixedPageAllocator),
    CapAllocator(CapAllocatorWrapper),
}

/// A reference to a page allocator that can be cheaply cloned
#[derive(Clone)]
pub struct PaRef(PaRefInner);

impl PaRef {
    pub fn zm() -> Self {
        PaRef(PaRefInner::PmemManager(zm()))
    }

    pub unsafe fn init_allocator(fixed_page_allocator: *const FixedPageAllocator) -> Self {
        PaRef(PaRefInner::InitAllocator(fixed_page_allocator))
    }

    pub fn cap_allocator(cap_allocator: CapAllocatorWrapper) -> Self {
        PaRef(PaRefInner::CapAllocator(cap_allocator))
    }

    pub fn from_arc(allocator: Arc<CapAllocator>) -> Self {
        PaRef(PaRefInner::CapAllocator(allocator.into()))
    }

    pub fn alloc(&mut self, layout: PageLayout) -> Option<Allocation> {
        match self.0 {
            PaRefInner::PmemManager(pmem_manager) => pmem_manager.alloc(layout),
            PaRefInner::InitAllocator(init_allocator) => unsafe { (*init_allocator).alloc(layout) },
            PaRefInner::CapAllocator(ref mut cap_allocator) => cap_allocator.page_alloc(layout),
        }
    }

    pub unsafe fn dealloc(&mut self, allocation: Allocation) {
        unsafe {
            match self.0 {
                PaRefInner::PmemManager(pmem_manager) => pmem_manager.dealloc(allocation),
                PaRefInner::InitAllocator(init_allocator) => (*init_allocator).dealloc(allocation),
                PaRefInner::CapAllocator(ref mut cap_allocator) => cap_allocator.page_dealloc(allocation),
            }
        }
    }

    pub unsafe fn realloc(&mut self, allocation: Allocation, layout: PageLayout) -> Option<Allocation> {
        unsafe {
            match self.0 {
                PaRefInner::PmemManager(pmem_manager) => pmem_manager.realloc(allocation, layout),
                PaRefInner::InitAllocator(init_allocator) => (*init_allocator).realloc(allocation, layout),
                PaRefInner::CapAllocator(ref mut cap_allocator) => cap_allocator.page_realloc(allocation, layout),
            }
        }
    }

    pub unsafe fn realloc_in_place(&mut self, allocation: Allocation, layout: PageLayout) -> Option<Allocation> {
        unsafe {
            match self.0 {
                PaRefInner::PmemManager(pmem_manager) => pmem_manager.realloc_in_place(allocation, layout),
                PaRefInner::InitAllocator(init_allocator) => (*init_allocator).realloc_in_place(allocation, layout),
                PaRefInner::CapAllocator(ref mut cap_allocator) => cap_allocator.page_realloc_in_place(allocation, layout),
            }
        }
    }
}

unsafe impl Send for PaRef {}
unsafe impl Sync for PaRef {}

impl Debug for PaRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "(PaRef)")
    }
}