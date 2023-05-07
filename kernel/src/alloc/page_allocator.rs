use crate::make_alloc_ref;
use crate::mem::{Allocation, PageLayout};
use crate::prelude::*;

/// A trait that represents an object that can allocate physical memory pages
pub unsafe trait PageAllocator: Send + Sync {
    /// Allocates a page according to page layout
    fn alloc(&self, layout: PageLayout) -> Option<Allocation>;

    /// Deallocate pages, uses the zindex field as metadata to deallocate the allocation if it is not None
    unsafe fn dealloc(&self, allocation: Allocation);

    /// Reallocates the allocation to match the layout
    unsafe fn realloc(&self, allocation: Allocation, layout: PageLayout) -> Option<Allocation> {
        let mut out = self.alloc(layout)?;
        out.copy_from_mem(allocation.as_slice());
        unsafe {
            self.dealloc(allocation);
        }
        Some(out)
    }
}

make_alloc_ref!(PaRef, PaRefInner, PageAllocator);
