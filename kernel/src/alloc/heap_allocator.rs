use core::alloc::Layout;
use core::ops::Deref;
use core::fmt;

use crate::prelude::*;
use crate::mem::HeapAllocation;

/// A trait that represents an object that can allocate heap memory
pub trait HeapAllocator: Send + Sync {
	fn alloc(&self, layout: Layout) -> Option<HeapAllocation>;
	unsafe fn dealloc(&self, allocation: HeapAllocation);

	unsafe fn realloc(&self, allocation: HeapAllocation, layout: Layout) -> Option<HeapAllocation> {
		let mut mem = self.alloc(layout)?;
		mem.copy_from_mem(allocation.as_slice());
		self.dealloc(allocation);
		Some(mem)
	}
}

#[derive(Clone)]
enum AllocRefInner {
	Static(&'static dyn HeapAllocator),
	Raw(*const dyn HeapAllocator),
	// uncomment once Arcs are addded
	//OtherRc(Arc<CapAllocator>),
}

unsafe impl Send for AllocRefInner {}
unsafe impl Sync for AllocRefInner {}

impl fmt::Debug for AllocRefInner {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		writeln!(f, "(AllocRefInner)")
	}
}

/// A reference to a heap allocator
#[derive(Debug, Clone)]
pub struct AllocRef(AllocRefInner);

impl AllocRef {
	pub fn new(allocer: &'static dyn  HeapAllocator) -> Self {
		AllocRef(AllocRefInner::Static(allocer))
	}

	// FIXME: find a better solution
	// safety: object
	pub unsafe fn new_raw(allocer: *const dyn HeapAllocator) -> Self {
		AllocRef(AllocRefInner::Raw(allocer))
	}

	pub fn allocator(&self) -> &dyn HeapAllocator {
		self.deref()
	}
}

impl Deref for AllocRef {
	type Target = dyn HeapAllocator;

	fn deref(&self) -> &Self::Target {
		match self.0 {
			AllocRefInner::Static(allocer) => allocer,
			AllocRefInner::Raw(ptr) => unsafe { ptr.as_ref().unwrap() },
		}
	}
}
