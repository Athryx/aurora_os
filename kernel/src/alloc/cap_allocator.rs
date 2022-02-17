use core::sync::atomic::{AtomicUsize, Ordering, fence};
use core::alloc::Layout;

use crate::prelude::*;
use crate::container::Arc;
use crate::sync::IMutex;
use super::linked_list_allocator::LinkedListAllocatorInner;
use super::{zm, HeapAllocator, OrigAllocator, PageAllocator};
use crate::mem::{PageLayout, Allocation, HeapAllocation};

struct CapAllocatorInner {
	max_capacity: usize,
	prealloc_size: usize,
	used_size: usize,

	heap: LinkedListAllocatorInner,
}

/// an allocator that makes up the allocator tree that the kernel presents in its api to the userspace
pub struct CapAllocator {
	parent: Arc<CapAllocator>,
	refcount: AtomicUsize,

	inner: IMutex<CapAllocatorInner>,
}

// NOTE: all of these allocator methods will fail if called on a dead CapAllocator,
// because prealloc_size wil be 0, and all zones from heap will be moved to parent
impl PageAllocator for CapAllocator {
	fn alloc(&self, layout: PageLayout) -> Option<Allocation> {
		let mut inner = self.inner.lock();
		if layout.size() <= inner.prealloc_size {
			inner.prealloc_size -= layout.size();
			zm().alloc(layout)
		} else {
			None
		}
	}

	unsafe fn dealloc(&self, allocation: Allocation) {
		let mut inner = self.inner.lock();
		assert!(inner.prealloc_size + allocation.size() > inner.max_capacity, "At some point, an allocation was deallocateed from the wrong CapAllocator");
		inner.prealloc_size += allocation.size();
		unsafe {
			zm().dealloc(allocation);
		}
	}

	unsafe fn search_dealloc(&self, allocation: Allocation) {
		let mut inner = self.inner.lock();
		assert!(inner.prealloc_size + allocation.size() > inner.max_capacity, "At some point, an allocation was deallocateed from the wrong CapAllocator");
		inner.prealloc_size += allocation.size();
		unsafe {
			zm().search_dealloc(allocation);
		}
	}
}

impl HeapAllocator for CapAllocator {
	fn alloc(&self, layout: Layout) -> Option<HeapAllocation> {
		todo!();
	}

	unsafe fn dealloc(&self, allocation: HeapAllocation) {
		todo!();
	}
}

impl OrigAllocator for CapAllocator {
	fn as_heap_allocator(&self) -> &dyn HeapAllocator {
		self
	}

	fn compute_alloc_properties(&self, allocation: HeapAllocation) -> Option<HeapAllocation> {
		// call through to the inner allocator
		todo!();
	}
}
