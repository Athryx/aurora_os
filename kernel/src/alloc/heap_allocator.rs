use core::alloc::Layout;

use crate::prelude::*;
use crate::mem::HeapAllocation;
use crate::make_alloc_ref;

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

/// A trait that represents an allocator which can deallocate objects given the orginal size and align passed into alloc
pub trait OrigAllocator: HeapAllocator {
	/// This function takes in an allocation
	/// This allocation can have the same size and align properties as the layout the user allocated an object with before
	/// It will then return a HeapAllocation that has the actual size and align properties the allocator would have returned
	/// If it is impossible to compute because the address field is wrong, and the allocation could not have come from this allocator,
	/// It will return None
	fn compute_alloc_properties(&self, _allocation: HeapAllocation) -> Option<HeapAllocation>;

	unsafe fn dealloc_orig(&self, allocation: HeapAllocation) {
		if let Some(allocation) = self.compute_alloc_properties(allocation) {
			self.dealloc(allocation)
		}
	}

	unsafe fn realloc_orig(&self, allocation: HeapAllocation, layout: Layout) -> Option<HeapAllocation> {
		let allocation = self.compute_alloc_properties(allocation)?;
		self.realloc(allocation, layout)
	}
}

make_alloc_ref!(AllocRef, AllocRefInner, HeapAllocator);
make_alloc_ref!(OrigRef, OrigRefInner, OrigAllocator);

impl OrigRef {
	/// Returns an alloc ref referencing the same allocator the orig ref referenced
	pub fn downgrade(&self) -> AllocRef {
		match self.0 {
			OrigRefInner::Static(allocer) => AllocRef::new(allocer),
			OrigRefInner::Raw(ptr) => unsafe { AllocRef::new_raw(ptr) },
		}
	}
}
