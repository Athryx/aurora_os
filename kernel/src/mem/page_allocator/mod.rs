mod page_allocation;
mod page_layout;
mod fixed_page_allocator;
mod pmem_manager;

use core::fmt::{self, Debug};

use spin::Once;

use crate::container::Arc;
use crate::prelude::*;
use crate::mb2::MemoryMap;

pub use page_allocation::PageAllocation;
pub use page_layout::PageLayout;
pub use fixed_page_allocator::FixedPageAllocator;
pub use pmem_manager::PmemManager;
use super::tracking_allocator::{TrackingAllocator, TrackingAllocatorWrapper};

/// A trait that represents an object that can allocate physical memory pages
// NOTE: this isn't really necessary anymore now that PaRef is &mut self with these, just exists for documentation purposes
pub unsafe trait PageAllocator: Send + Sync {
    /// Allocates a page according to page layout
    fn alloc(&self, layout: PageLayout) -> Option<PageAllocation>;

    /// Deallocate pages, uses the zindex field as metadata to deallocate the allocation if it is not None
    unsafe fn dealloc(&self, allocation: PageAllocation);

    /// Reallocates the allocation to match the layout
    unsafe fn realloc(&self, allocation: PageAllocation, layout: PageLayout) -> Option<PageAllocation> {
        let mut out = self.alloc(layout)?;
        unsafe {
            // safety: allocations do not overlap because alloc will ensure they don't overlap
            out.copy_from_mem(allocation.as_slice_ptr());
            self.dealloc(allocation);
        }
        Some(out)
    }

    unsafe fn realloc_in_place(&self, _allocation: PageAllocation, _layout: PageLayout) -> Option<PageAllocation> {
        None
    }
}

// this is in inner enum so InitAllocator cannot be constructed without unsafe
#[derive(Clone)]
enum PaRefInner {
    PmemManager(&'static PmemManager),
    InitAllocator(*const FixedPageAllocator),
    TrackingAllocator(TrackingAllocatorWrapper),
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

    pub fn tracking_allocator(cap_allocator: TrackingAllocatorWrapper) -> Self {
        PaRef(PaRefInner::TrackingAllocator(cap_allocator))
    }

    pub fn from_arc(allocator: Arc<TrackingAllocator>) -> Self {
        PaRef(PaRefInner::TrackingAllocator(allocator.into()))
    }

    pub fn alloc(&mut self, layout: PageLayout) -> Option<PageAllocation> {
        match self.0 {
            PaRefInner::PmemManager(pmem_manager) => pmem_manager.alloc(layout),
            PaRefInner::InitAllocator(init_allocator) => unsafe { (*init_allocator).alloc(layout) },
            PaRefInner::TrackingAllocator(ref mut cap_allocator) => cap_allocator.page_alloc(layout),
        }
    }

    pub unsafe fn dealloc(&mut self, allocation: PageAllocation) {
        unsafe {
            match self.0 {
                PaRefInner::PmemManager(pmem_manager) => pmem_manager.dealloc(allocation),
                PaRefInner::InitAllocator(init_allocator) => (*init_allocator).dealloc(allocation),
                PaRefInner::TrackingAllocator(ref mut cap_allocator) => cap_allocator.page_dealloc(allocation),
            }
        }
    }

    pub unsafe fn realloc(&mut self, allocation: PageAllocation, layout: PageLayout) -> Option<PageAllocation> {
        unsafe {
            match self.0 {
                PaRefInner::PmemManager(pmem_manager) => pmem_manager.realloc(allocation, layout),
                PaRefInner::InitAllocator(init_allocator) => (*init_allocator).realloc(allocation, layout),
                PaRefInner::TrackingAllocator(ref mut cap_allocator) => cap_allocator.page_realloc(allocation, layout),
            }
        }
    }

    pub unsafe fn realloc_in_place(&mut self, allocation: PageAllocation, layout: PageLayout) -> Option<PageAllocation> {
        unsafe {
            match self.0 {
                PaRefInner::PmemManager(pmem_manager) => pmem_manager.realloc_in_place(allocation, layout),
                PaRefInner::InitAllocator(init_allocator) => (*init_allocator).realloc_in_place(allocation, layout),
                PaRefInner::TrackingAllocator(ref mut cap_allocator) => cap_allocator.page_realloc_in_place(allocation, layout),
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

pub(super) static PMEM_MANAGER: Once<PmemManager> = Once::new();

/// Returns the zone manager (which manages all physical pages on the system)
/// 
/// # Panics
/// Panics if the zone manager has not yet been initialized
pub fn zm() -> &'static PmemManager {
    PMEM_MANAGER
        .get()
        .expect("zone manager (PmemManager) has not been initilized")
}

/// Initializes the kernel page allocator, and returns the number of allocatable pages
pub fn init(mem_map: &MemoryMap) -> usize {
    let mut total_pages = 0;
    PMEM_MANAGER.call_once(|| {
        let (pmem_manager, pages) = unsafe { PmemManager::new(mem_map) };
        total_pages = pages;
        pmem_manager
    });

    total_pages
}