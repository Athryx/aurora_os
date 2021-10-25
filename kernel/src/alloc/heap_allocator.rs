use core::alloc::Layout;

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
