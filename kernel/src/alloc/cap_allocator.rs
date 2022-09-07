use core::sync::atomic::{AtomicBool, Ordering};
use core::alloc::Layout;

use spin::Once;

use crate::prelude::*;
use crate::cap::CapObject;
use crate::container::Arc;
use crate::sync::{IMutex, IMutexGuard};
use super::linked_list_allocator::LinkedListAllocatorInner;
use super::{zm, HeapAllocator, OrigAllocator, PageAllocator};
use crate::mem::{PageLayout, Allocation, HeapAllocation};

struct CapAllocatorPageData {
	max_capacity: usize,
	prealloc_size: usize,
	used_size: usize,
}

pub struct RootAllocator {
	page_data: IMutex<CapAllocatorPageData>,
	heap: IMutex<LinkedListAllocatorInner>,
}

impl RootAllocator {
	pub fn new(total_pages: usize) -> Self {
		RootAllocator {
			page_data: IMutex::new(CapAllocatorPageData {
				max_capacity: total_pages * PAGE_SIZE,
				prealloc_size: total_pages * PAGE_SIZE,
				used_size: 0,
			}),
			heap: IMutex::new(LinkedListAllocatorInner::new()),
		}
	}
}

impl CapObject for RootAllocator {
	fn cap_drop(&self) {}
}

impl PageAllocator for RootAllocator {
	fn alloc(&self, layout: PageLayout) -> Option<Allocation> {
		let mut page_data = self.page_data.lock();
		if layout.size() <= page_data.prealloc_size {
			page_data.prealloc_size -= layout.size();
			zm().alloc(layout)
		} else {
			None
		}
	}

	unsafe fn dealloc(&self, allocation: Allocation) {
		let mut page_data = self.page_data.lock();
		assert!(page_data.prealloc_size + allocation.size() <= page_data.max_capacity, "At some point, an allocation was deallocateed from the wrong CapAllocator");
		page_data.prealloc_size += allocation.size();
		unsafe {
			zm().dealloc(allocation);
		}
	}

	unsafe fn search_dealloc(&self, allocation: Allocation) {
		let mut page_data = self.page_data.lock();
		assert!(page_data.prealloc_size + allocation.size() <= page_data.max_capacity, "At some point, an allocation was deallocateed from the wrong CapAllocator");
		page_data.prealloc_size += allocation.size();
		unsafe {
			zm().search_dealloc(allocation);
		}
	}
}

impl HeapAllocator for RootAllocator {
	fn alloc(&self, layout: Layout) -> Option<HeapAllocation> {
		self.heap.lock().alloc(layout, self)
	}

	unsafe fn dealloc(&self, allocation: HeapAllocation) {
		unsafe {
			self.heap.lock().dealloc(allocation)
		}
	}
}

impl OrigAllocator for RootAllocator {
	fn as_heap_allocator(&self) -> &dyn HeapAllocator {
		self
	}

	fn compute_alloc_properties(&self, allocation: HeapAllocation) -> Option<HeapAllocation> {
		LinkedListAllocatorInner::compute_alloc_properties(allocation)
	}
}

pub enum CapAllocatorParent {
	Normal(Arc<CapAllocator>),
	Root(&'static RootAllocator),
}

/// an allocator that makes up the allocator tree that the kernel presents in its api to the userspace
pub struct CapAllocator {
	parent: IMutex<CapAllocatorParent>,
	is_alive: AtomicBool,

	page_data: IMutex<CapAllocatorPageData>,
	heap: IMutex<LinkedListAllocatorInner>,
}

impl CapAllocator {
	pub fn is_alive(&self) -> bool {
		self.is_alive.load(Ordering::Acquire)
	}

	// TODO: remove recurse depth
	// this is a temporary hack to stop malicous processess causing a kernel stack overflow
	// try to find a better way to avoid stack overflow without limiting prealloc depth
	fn prealloc_inner(&self, page_data_lock: IMutexGuard<CapAllocatorPageData>, pages: usize, recurse_depth: &mut usize) -> KResult<()> {
		let mut parent_lock = self.parent.lock();
		let mut parent: Arc<CapAllocator>;
		*parent_lock = loop {
			match &*parent_lock {
				CapAllocatorParent::Normal(ref new_parent) => {
					parent = new_parent.clone();
					if new_parent.is_alive() {
						break CapAllocatorParent::Normal(parent);
					}

					parent_lock = parent.parent.lock();
				}
				CapAllocatorParent::Root(parent) => break CapAllocatorParent::Root(parent),
			}
		};
		todo!()
	}
}

impl Arc<CapAllocator> {
	pub fn get_closest_alive_parent(&self) -> CapAllocatorParent {
		let mut current = self.clone();
		loop {
			if current.is_alive() {
				return CapAllocatorParent::Normal(current.clone());
			}

			let new_current = match &*current.parent.lock() {
				CapAllocatorParent::Normal(parent) => parent.clone(),
				CapAllocatorParent::Root(parent) => return CapAllocatorParent::Root(parent),
			};
			current = new_current;
		}
	}
}

impl CapObject for CapAllocator {
	fn cap_drop(&self) {
		// TODO: move heap zones to parent when dying
		let mut page_data = self.page_data.lock();
		page_data.prealloc_size = 0;
		self.is_alive.store(false, Ordering::Release);
	}
}

// NOTE: all of these allocator methods will fail if called on a dead CapAllocator,
// because prealloc_size wil be 0, and all zones from heap will be moved to parent
impl PageAllocator for CapAllocator {
	fn alloc(&self, layout: PageLayout) -> Option<Allocation> {
		let mut page_data = self.page_data.lock();
		if layout.size() <= page_data.prealloc_size {
			page_data.prealloc_size -= layout.size();
			zm().alloc(layout)
		} else {
			None
		}
	}

	unsafe fn dealloc(&self, allocation: Allocation) {
		let mut page_data = self.page_data.lock();
		assert!(page_data.prealloc_size + allocation.size() <= page_data.max_capacity, "At some point, an allocation was deallocateed from the wrong CapAllocator");
		page_data.prealloc_size += allocation.size();
		unsafe {
			zm().dealloc(allocation);
		}
	}

	unsafe fn search_dealloc(&self, allocation: Allocation) {
		let mut page_data = self.page_data.lock();
		assert!(page_data.prealloc_size + allocation.size() <= page_data.max_capacity, "At some point, an allocation was deallocateed from the wrong CapAllocator");
		page_data.prealloc_size += allocation.size();
		unsafe {
			zm().search_dealloc(allocation);
		}
	}
}

impl HeapAllocator for CapAllocator {
	fn alloc(&self, layout: Layout) -> Option<HeapAllocation> {
		self.heap.lock().alloc(layout, self)
	}

	unsafe fn dealloc(&self, allocation: HeapAllocation) {
		unsafe {
			self.heap.lock().dealloc(allocation)
		}
	}
}

impl OrigAllocator for CapAllocator {
	fn as_heap_allocator(&self) -> &dyn HeapAllocator {
		self
	}

	fn compute_alloc_properties(&self, allocation: HeapAllocation) -> Option<HeapAllocation> {
		LinkedListAllocatorInner::compute_alloc_properties(allocation)
	}
}
