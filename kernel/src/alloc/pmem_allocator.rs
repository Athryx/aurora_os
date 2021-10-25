use core::sync::atomic::{AtomicUsize, AtomicU8, Ordering};
use core::slice;
use core::cmp::min;

use bitflags::bitflags;

use crate::prelude::*;
use crate::mem::Allocation;

bitflags! {
	struct TreeStatus: u8 {
		const OCCUPY_RIGHT = 1;
		const OCCUPY_LEFT = 1 << 1;
		const COAL_RIGHT = 1 << 2;
		const COAL_LEFT = 1 << 3;
		const OCCUPY = 1 << 4;
		const BUSY = Self::OCCUPY_RIGHT.bits | Self::OCCUPY_LEFT.bits | Self::OCCUPY.bits;
	}
}

impl TreeStatus {
	fn get_coal(&mut self, child: Child) -> bool {
		match child {
			Child::Left => self.contains(Self::COAL_LEFT),
			Child::Right => self.contains(Self::COAL_RIGHT),
		}
	}

	fn set_coal(&mut self, child: Child) {
		match child {
			Child::Left => self.insert(Self::COAL_LEFT),
			Child::Right => self.insert(Self::COAL_RIGHT),
		}
	}

	fn clear_coal(&mut self, child: Child) {
		match child {
			Child::Left => self.remove(Self::COAL_LEFT),
			Child::Right => self.remove(Self::COAL_RIGHT),
		}
	}

	fn get_occupy(&mut self, child: Child) -> bool {
		match child {
			Child::Left => self.contains(Self::OCCUPY_LEFT),
			Child::Right => self.contains(Self::OCCUPY_RIGHT),
		}
	}

	fn set_occupy(&mut self, child: Child) {
		match child {
			Child::Left => self.insert(Self::OCCUPY_LEFT),
			Child::Right => self.insert(Self::OCCUPY_RIGHT),
		}
	}

	fn clear_occupy(&mut self, child: Child) {
		match child {
			Child::Left => self.remove(Self::OCCUPY_LEFT),
			Child::Right => self.remove(Self::OCCUPY_RIGHT),
		}
	}
}

#[derive(Debug, Clone, Copy)]
enum Child {
	Left,
	Right,
}

impl Child {
	fn buddy(&self) -> Self {
		match self {
			Self::Left => Self::Right,
			Self::Right => Self::Left,
		}
	}
}

impl From<usize> for Child {
	fn from(n: usize) -> Self {
		assert_ne!(n, 0);
		match n % 2 {
			0 => Self::Right,
			1 => Self::Left,
			_ => unreachable!(),
		}
	}
}

#[derive(Debug, Clone, Copy)]
struct TreeNode<'a> {
	allocator: &'a PmemAllocator,
	index: usize,
}

impl<'a> TreeNode<'a> {
	fn left(&self) -> TreeNode<'a> {
		self.clone_index(2 * (self.index + 1) - 1)
	}

	fn right(&self) -> TreeNode<'a> {
		self.clone_index(2 * (self.index + 1))
	}

	fn parent(&self) -> Option<TreeNode<'a>> {
		if self.index == 0 {
			None
		} else {
			Some(self.clone_index((self.index - 1) / 2))
		}
	}

	fn child_type(&self) -> Child {
		self.index.into()
	}

	fn is_free(&self) -> bool {
		(self.data().load(Ordering::Acquire) & TreeStatus::BUSY.bits()) == 0
	}

	fn data(&self) -> &'a AtomicU8 {
		&self.allocator.tree_slice()[self.index]
	}

	// the address in memory that this node references
	fn addr(&self) -> usize {
		// start index of this node's level
		let start_index = (1 << self.level()) - 1;
		let diff = self.index - start_index;
		self.allocator.start_addr + self.size() * diff
	}

	// size of memory this node refernces
	fn size(&self) -> usize {
		self.allocator.max_size / (1 << self.level())
	}

	fn level(&self) -> usize {
		log2(self.index + 1)
	}

	fn clone_index(&self, index: usize) -> TreeNode<'a> {
		TreeNode {
			allocator: self.allocator,
			index,
		}
	}
}

#[derive(Debug)]
pub struct PmemAllocator {
	// start and end address of allocatable memory
	start_addr: usize,
	end_addr: usize,

	// pointer and length of tree array
	tree: *const [AtomicU8],

	// pointer and length of index arrray
	index: *const AtomicUsize,
	index_len: usize,

	// maximum depth of the tree
	depth: usize,
	// maximum size that can be allocated, size of the root node
	max_size: usize,
	// minimum size of a level
	level_size: usize,
	// amount of free memory available
	// TODO: track size left
	free_space: AtomicUsize,
}

impl PmemAllocator {

	pub fn new(addr: usize, size: usize, level_size: usize) -> Self {
		todo!();
	}

	pub fn alloc(&self, size: usize) -> Option<Allocation> {
		if size > self.max_size {
			return None;
		}

		// get level that is big enough to hold size
		let level = min(log2(self.max_size / size), self.depth);

		// iterate over all nodes in the correct level
		let mut i = (1 << level) - 1;
		let end = (1 << (level + 1)) - 1;

		while i < end {
			let node = self.get_tree_node(i);
			if node.is_free() {
				if let Some(fail_node) = self.try_alloc(node) {
					// allocation failed, move to next valid node

					// how much bigger the node is that we failed at compared to the current level
					let scale = 1 << (node.level() - fail_node.level());

					// use same formula as get left
					// this node is the leftmost child of the node we failed at in the current level
					let level_start_index = scale * (fail_node.index + 1) - 1;

					i = level_start_index + scale;
					continue;
				} else {
					// allocation succeeded
					//self.index_slice()[node.addr()].store(node.index, Ordering::Release);
					self.free_space.fetch_sub(node.size(), Ordering::Relaxed);
					return Some(Allocation::new(node.addr(), node.size()));
				}
			}

			i += 1;
		}

		None
	}

	// returns Some(node) on failure, where node is the TreeNode that caused a problem
	fn try_alloc<'a> (&'a self, node: TreeNode<'a>) -> Option<TreeNode<'a>> {
		if node.data().compare_exchange(0, TreeStatus::BUSY.bits(), Ordering::AcqRel, Ordering::Relaxed).is_err() {
			return Some(node);
		}

		let mut current = node;

		loop {
			let child = current;

			// run this code before child type because this needs to exit the function when the node is the parent
			current = current.parent()?;

			let child_type = child.child_type();

			let res = current.data().fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
				let mut flags = unsafe { TreeStatus::from_bits_unchecked(n) };

				if flags.contains(TreeStatus::OCCUPY) {
					return None;
				}

				flags.clear_coal(child_type);
				flags.set_occupy(child_type);

				Some(flags.bits())
			});

			if res.is_err() {
				self.dealloc_node(node, child);
				return Some(current);
			}
		}
	}

	// attempts to adjust allocation to match new_size, returns none on failure
	// if it fails, the old allocation is left unchanged
	// TODO: more intelligent reallocation
	/*pub unsafe fn realloc(&self, allocation: Allocation, new_size: usize) -> Option<Allocation> {
		let new = self.alloc(new_size)?;

		todo!();
	}*/

	pub unsafe fn dealloc(&self, allocation: Allocation) {
		// get level that is big enough to hold size
		let level = log2(self.max_size / allocation.size());
		assert!(level <= self.depth);

		let level_start = (1 << level) - 1;

		let addr_offset = allocation.as_usize() - self.start_addr;

		let node = self.get_tree_node(level_start + (addr_offset / allocation.size()));

		self.dealloc_node(node, self.get_tree_node(0));

		self.free_space.fetch_add(node.size(), Ordering::Relaxed);
	}

	// goes up the tree starting from start, and up to and including end
	fn dealloc_node(&self, start: TreeNode, end: TreeNode) {
		let mut current = start;

		while current.level() > end.level() {
			let child = current;
			let child_type = child.child_type();
			let buddy_type = child_type.buddy();

			// panic safety: because the highest node end can be is the root node, this will never panic
			// because if current is root the while loop condition will fail
			current = current.parent().unwrap();

			let mut flags = TreeStatus::empty();

			current.data().fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
				flags = unsafe { TreeStatus::from_bits_unchecked(n) };
				flags.clear_coal(child_type);
				Some(flags.bits())
			}).unwrap();

			if flags.get_occupy(buddy_type) && !flags.get_coal(buddy_type) {
				break;
			}
		}

		start.data().store(TreeStatus::empty().bits(), Ordering::Release);

		let mut current = start;

		while current.level() > end.level() {
			let child = current;
			let child_type = child.child_type();
			let buddy_type = child_type.buddy();

			// panic safety: because the highest node end can be is the root node, this will never panic
			// because if current is root the while loop condition will fail
			current = current.parent().unwrap();

			let mut flags = TreeStatus::empty();

			let res = current.data().fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
				flags = unsafe { TreeStatus::from_bits_unchecked(n) };

				if flags.get_coal(child_type) {
					flags.clear_occupy(child_type);
					flags.clear_coal(child_type);
					Some(flags.bits())
				} else {
					None
				}
			});

			if res.is_err() || flags.get_occupy(buddy_type) {
				break;
			}
		}
	}

	fn get_tree_node(&self, index: usize) -> TreeNode {
		TreeNode {
			allocator: self,
			index,
		}
	}

	fn tree_slice(&self) -> &[AtomicU8] {
		unsafe {
			self.tree.as_ref().unwrap()
		}
	}

	fn index_slice(&self) -> &[AtomicUsize] {
		unsafe {
			slice::from_raw_parts(self.index, self.index_len)
		}
	}
}

unsafe impl Send for PmemAllocator {}
unsafe impl Sync for PmemAllocator {}
