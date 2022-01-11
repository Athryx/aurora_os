use crate::prelude::*;
use crate::mem::{Allocation, PageLayout};
use crate::make_alloc_ref;

/// A trait that represents an object that can allocate physical memory pages
pub trait PageAllocator: Send + Sync {
	/// Allocates a page according to page layout
	fn alloc(&self, layout: PageLayout) -> Option<Allocation>;

	/// Deallocate pages, uses the zindex field as metadata to deallocate the allocation
	unsafe fn dealloc(&self, allocation: Allocation) {
		unsafe {
			self.search_dealloc(allocation);
		}
	}

	/// Deallocate pages, does not use the zindex field as metadata
	/// Useful if it is inpractical to store the zindex field, such as in page tables, but it is slightly slower
	unsafe fn search_dealloc(&self, allocation: Allocation);

	/// Reallocates the allocation to match the layout
	unsafe fn realloc(&self, allocation: Allocation, layout: PageLayout) -> Option<Allocation> {
		let mut out = self.alloc(layout)?;
		out.copy_from_mem(allocation.as_slice());
		unsafe {
			self.dealloc(allocation);
		}
		Some(out)
	}

	/// Reallocates the allocation to match the layout, does not use the zindex field as metadata
	/// Useful if it is inpractical to store the zindex field, such as in page tables, but it is slightly slower
	unsafe fn search_realloc(&self, allocation: Allocation, layout: PageLayout) -> Option<Allocation> {
		let mut out = self.alloc(layout)?;
		out.copy_from_mem(allocation.as_slice());
		unsafe {
			self.search_dealloc(allocation);
		}
		Some(out)
	}
}

make_alloc_ref!(PaRef, PaRefInner, PageAllocator);
