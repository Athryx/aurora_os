use core::alloc::Layout;
use core::ops::Deref;

use crate::prelude::*;
use crate::mem::HeapAllocation;

pub trait HeapAllocator {
	fn alloc(&self, layout: Layout) -> Option<HeapAllocation>;
	unsafe fn dealloc(&self, allocation: HeapAllocation);

	unsafe fn realloc(&self, allocation: HeapAllocation, layout: Layout) -> Option<HeapAllocation> {
		let mut mem = self.alloc(layout)?;
		mem.copy_from_mem(allocation.as_slice());
		self.dealloc(allocation);
		Some(mem)
	}
}

enum AllocRefInner {
	Static(&'static dyn HeapAllocator),
	Raw(*const dyn HeapAllocator),
	// uncomment once Arcs are addded
	//OtherRc(Arc<CapAllocator>),
}

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
