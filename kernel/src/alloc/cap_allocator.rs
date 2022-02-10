use crate::prelude::*;
use super::linked_list_allocator::LinkedListAllocator;

/// an allocator that makes up the allocator tree that the kernel presents in its api to the userspace
pub struct CapAllocator {
	//parent: Arc<CapAllocator>,

	max_capacity: usize,
	prealloc_size: usize,
	used_size: usize,

	heap: LinkedListAllocator,
}
