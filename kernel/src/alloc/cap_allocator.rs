use core::sync::atomic::{AtomicBool, Ordering};
use core::alloc::Layout;

use spin::Once;

use crate::prelude::*;
use crate::cap::CapObject;
use crate::container::Arc;
use crate::sync::{IMutex, IMutexGuard};
use super::heap;
use super::linked_list_allocator::LinkedListAllocatorInner;
use super::{zm, HeapAllocator, OrigAllocator, PageAllocator};
use crate::mem::{PageLayout, Allocation, HeapAllocation};

struct CapAllocatorPageData {
	max_capacity: usize,
	prealloc_size: usize,
	used_size: usize,
}

/// an allocator that makes up the allocator tree that the kernel presents in its api to the userspace
pub struct CapAllocator {
	is_alive: AtomicBool,
	parent: Option<IMutex<Arc<CapAllocator>>>,
	page_data: IMutex<CapAllocatorPageData>,
}

impl CapAllocator {
	pub fn new_root(total_pages: usize) -> Self {
		Self {
			is_alive: AtomicBool::new(true),
			parent: None,
			page_data: IMutex::new(CapAllocatorPageData {
				max_capacity: PAGE_SIZE * total_pages,
				prealloc_size: PAGE_SIZE * total_pages,
				used_size: 0,
			}),
		}
	}

	pub fn is_alive(&self) -> bool {
		self.is_alive.load(Ordering::Acquire)
	}

	pub fn is_root(&self) -> bool {
		self.parent.is_none()
	}

	const PREALLOC_RECURSE_DEPTH: usize = 8;

	// TODO: remove recurse depth
	// TODO: implement all extra features of prealloc
	// this is a temporary hack to stop malicous processess causing a kernel stack overflow
	// try to find a better way to avoid stack overflow without limiting prealloc depth
	fn prealloc_inner(&self, page_data_lock: &mut IMutexGuard<CapAllocatorPageData>, bytes: usize, recurse_depth: &mut usize) -> KResult<()> {
		*recurse_depth -= 1;
		if *recurse_depth == 0 {
			// FIXME:
			return Err(SysErr::Unknown)
		}

		if page_data_lock.used_size + page_data_lock.prealloc_size + bytes > page_data_lock.max_capacity {
			return Err(SysErr::OutOfMem)
		}

		let parent_lock = match self.parent.as_ref() {
			Some(parent) => parent,
			// If this is the root node, we can never prealloc, so we are out of memory
			None => return Err(SysErr::OutOfMem),
		};

		let parent_lock_guard = parent_lock.lock();
		let mut parent = parent_lock_guard.clone().get_closest_alive_parent();

		let mut parent_page_data = parent.page_data.lock();

		// if parent doesn't have enough prealloced memory for us to take, ask them to prealloc
		if bytes > parent_page_data.prealloc_size {
			let prealloc_size = align_up(bytes - parent_page_data.prealloc_size, PAGE_SIZE);
			parent.prealloc_inner(
				&mut parent_page_data,
				prealloc_size,
				recurse_depth,
			)?;
		}
		
		parent_page_data.prealloc_size -= bytes;
		parent_page_data.used_size += bytes;
		page_data_lock.prealloc_size += bytes;

		Ok(())
	}

	// mark bytes as allocated from the allocator, returns out of mem on failure
	pub fn alloc_bytes(&self, bytes: usize) -> KResult<()> {
		let mut page_data = self.page_data.lock();

		if bytes > page_data.prealloc_size {
			let prealloc_size = align_up(bytes - page_data.prealloc_size, PAGE_SIZE);
			let mut recurse_depth = Self::PREALLOC_RECURSE_DEPTH;

			self.prealloc_inner(
				&mut page_data,
				prealloc_size,
				&mut recurse_depth,
			)?;
		}

		page_data.prealloc_size -= bytes;
		page_data.used_size += bytes;

		Ok(())
	}

	// marks bytes as dealloced
	pub fn dealloc_bytes(&self, bytes: usize) {
		let mut page_data = self.page_data.lock();
		assert!(page_data.used_size >= bytes, "tried to free to many bytes from this allocator");
		page_data.prealloc_size += bytes;
		page_data.used_size -= bytes;
	}
}

impl Arc<CapAllocator> {
	pub fn get_closest_alive_parent(self) -> Arc<CapAllocator> {
		let mut current = self;
		loop {
			if current.is_alive() {
				return current;
			}

			let new_lock = current.parent.as_ref().expect("root allocator died").lock();
			let new_parent = new_lock.clone();
			drop(new_lock);
			current = new_parent;
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
		let allocation = zm().alloc(layout)?;
		let result = self.alloc_bytes(allocation.size());
		if result.is_err() {
			unsafe {
				zm().dealloc(allocation);
			}
			None
		} else {
			Some(allocation)
		}
	}

	unsafe fn dealloc(&self, allocation: Allocation) {
		self.dealloc_bytes(allocation.size());
		unsafe {
			zm().dealloc(allocation);
		}
	}

	unsafe fn search_dealloc(&self, allocation: Allocation) {
		self.dealloc_bytes(allocation.size());
		unsafe {
			zm().search_dealloc(allocation);
		}
	}
}

impl HeapAllocator for CapAllocator {
	fn alloc(&self, layout: Layout) -> Option<HeapAllocation> {
		let allocation = heap().alloc(layout)?;
		let result = self.alloc_bytes(allocation.size());
		if result.is_err() {
			unsafe {
				heap().dealloc(allocation);
			}
			None
		} else {
			Some(allocation)
		}
	}

	unsafe fn dealloc(&self, allocation: HeapAllocation) {
		self.dealloc_bytes(allocation.size());
		unsafe {
			heap().dealloc(allocation)
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
