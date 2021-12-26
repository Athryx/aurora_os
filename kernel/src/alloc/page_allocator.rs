use core::ops::Deref;
use core::fmt;

use crate::prelude::*;
use crate::mem::{Allocation, PageLayout};

/// A trait that represents an object that can allocate physical memory pages
pub trait PageAllocator: Send + Sync {
	/// Allocates a page according to page layout
	fn alloc(&self, layout: PageLayout) -> Option<Allocation>;

	/// Deallocate pages, uses the zindex field as metadata to deallocate the allocation
	unsafe fn dealloc(&self, allocation: Allocation) {
		self.search_dealloc(allocation);
	}

	/// Deallocate pages, does not use the zindex field as metadata
	/// Useful if it is inpractical to store the zindex field, such as in page tables, but it is slightly slower
	unsafe fn search_dealloc(&self, allocation: Allocation);

	/// Reallocates the allocation to match the layout
	unsafe fn realloc(&self, allocation: Allocation, layout: PageLayout) -> Option<Allocation> {
		let mut out = self.alloc(layout)?;
		out.copy_from_mem(allocation.as_slice());
		self.dealloc(allocation);
		Some(out)
	}

	/// Reallocates the allocation to match the layout, does not use the zindex field as metadata
	/// Useful if it is inpractical to store the zindex field, such as in page tables, but it is slightly slower
	unsafe fn search_realloc(&self, allocation: Allocation, layout: PageLayout) -> Option<Allocation> {
		let mut out = self.alloc(layout)?;
		out.copy_from_mem(allocation.as_slice());
		self.search_dealloc(allocation);
		Some(out)
	}
}

#[derive(Clone)]
enum PaRefInner {
	Static(&'static dyn PageAllocator),
	Raw(*const dyn PageAllocator),
	// uncomment once Arcs are addded
	//OtherRc(Arc<CapAllocator>),
}

unsafe impl Send for PaRefInner {}
unsafe impl Sync for PaRefInner {}

impl fmt::Debug for PaRefInner {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		writeln!(f, "(AllocRefInner)")
	}
}

/// A reference to a page allocator
#[derive(Debug, Clone)]
pub struct PaRef(PaRefInner);

impl PaRef {
	pub fn new(allocer: &'static dyn  PageAllocator) -> Self {
		PaRef(PaRefInner::Static(allocer))
	}

	// FIXME: find a better solution
	// safety: object
	pub unsafe fn new_raw(allocer: *const dyn PageAllocator) -> Self {
		PaRef(PaRefInner::Raw(allocer))
	}

	pub fn allocator(&self) -> &dyn PageAllocator {
		self.deref()
	}
}

impl Deref for PaRef {
	type Target = dyn PageAllocator;

	fn deref(&self) -> &Self::Target {
		match self.0 {
			PaRefInner::Static(allocer) => allocer,
			PaRefInner::Raw(ptr) => unsafe { ptr.as_ref().unwrap() },
		}
	}
}
