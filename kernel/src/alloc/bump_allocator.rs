use core::alloc::Layout;

use crate::prelude::*;
use super::HeapAllocator;
use crate::mem::HeapAllocation;
use crate::sync::IMutex;

struct BumpInner {
	// current free address, inclusive
	current: usize,
	// end address, exlusive
	end: usize,
}

// used when initializing physical memory allocator
// temporary, will probably replaced with a real allocator once I write that
pub struct BumpAllocator {
	inner: IMutex<BumpInner>,
}

impl BumpAllocator {
	pub fn new(range: UVirtRange) -> Self {
		let inner = IMutex::new(BumpInner {
			current: range.as_usize(),
			end: range.as_usize() + range.size(),
		});

		BumpAllocator {
			inner,
		}
	}
}

impl HeapAllocator for BumpAllocator {
	fn alloc(&self, layout: Layout) -> Option<HeapAllocation> {
		let mut inner = self.inner.lock();

		let alloc_addr = align_up(inner.current, layout.align());
		let end_addr = alloc_addr + layout.size();

		if end_addr > inner.end {
			None
		} else {
			inner.current = end_addr;
			Some(HeapAllocation::from_layout(alloc_addr, layout))
		}
	}

	unsafe fn dealloc(&self, allocation: HeapAllocation) {
		let mut inner = self.inner.lock();

		if allocation.end_addr() == inner.end {
			inner.current -= allocation.size()
		}
	}
}
