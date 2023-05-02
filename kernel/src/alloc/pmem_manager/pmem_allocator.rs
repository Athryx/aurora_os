use core::cmp::min;
use core::slice;
use core::sync::atomic::{AtomicU8, AtomicUsize, Ordering};

use bitflags::bitflags;

use crate::mem::Allocation;
use crate::prelude::*;

bitflags! {
    struct TreeStatus: u8 {
        /// This node has memory in use in any of its right children or descendants
        const OCCUPY_RIGHT = 1;
        /// This node has memory in use in any of its left children or descendants
        const OCCUPY_LEFT = 1 << 1;
        /// This node has a memory release operation currently occuring in its right section
        const COAL_RIGHT = 1 << 2;
        /// This node has a memory release operation currently occuring in its left section
        const COAL_LEFT = 1 << 3;
        /// This entire node is currently allocated
        const OCCUPY = 1 << 4;
        /// When a node is allocated, it has all of these bits set
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
        self.allocator.start_addr() + self.size() * diff
    }

    // size of memory this node refernces
    fn size(&self) -> usize {
        self.allocator.max_size / (1 << self.level())
    }

    fn virt_range(&self) -> AVirtRange {
        AVirtRange::new(VirtAddr::new(self.addr()), self.size())
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

/// An atomic buddy allocator based on https://arxiv.org/pdf/1804.03436.pdf
/// 
/// Basically, this is a boddy allocator that has a series of levels, with the root level being the largest,
/// and each subsequent level being half the size of previous
/// An array called index uses status bits to track allocation status
/// 
/// Allocating an entry involves finding a free node at a given level, and going up the buddy tree,
/// marking each node as having an occupied child and clearing the coalese bit to cancel any deallocations that might remove these marks
/// If a fully occupied node is encountered, allocation is canceled and set to free
/// 
/// Deallocating involves first going up the tree and marking the coalese bit of all nodes where we are the only child on either the left or right side
/// then a second pass is made where we go up the tree and mark all the nodes with the coalese bit still set as free
#[derive(Debug)]
pub struct PmemAllocator {
    /// range of memory this allocator controls
    pub addr_range: AVirtRange,

    /// tree slice stores metadata about allocated memory
    pub tree: *const [AtomicU8],

    /// index slice stores allocated size of each index
    // TODO: remove because unused
    pub index: *const [AtomicUsize],

    /// maximum depth of the tree
    /// depth of 0 is the root node, and each subsequent depth has nodes that are half the size of previous
    depth: usize,
    /// maximum size that can be allocated, size of the root node
    max_size: usize,
    /// minimum size of a level, size of node at maximum depth
    level_size: usize,
    // amount of free memory available
    free_space: AtomicUsize,
}

impl PmemAllocator {
    /// Returns the required tree size and index size in bytes in a tuple, or none if vrange and level_size are not valid for the allocator
    pub fn required_tree_index_size(vrange: AVirtRange, level_size: usize) -> Option<(usize, usize)> {
        if level_size.is_power_of_two()
			&& vrange.is_power2_size_align()
			// because vrange size and level_size are both power of 2, they are guarenteed to divide eachother evenly
			&& vrange.size() >= level_size
        {
            Some((
                2 * (vrange.size() / level_size) - 1,
                size_of::<usize>() * vrange.size() / level_size,
            ))
        } else {
            None
        }
    }

    /// creates a new physical memory allocator, and panics if the invariants are not upheld
    ///
    /// # Safety
    /// must not have a mutable reference to tree or index alive once you start calling other allocator methods
    pub unsafe fn from(
        vrange: AVirtRange,
        tree: *mut [AtomicU8],
        index: *mut [AtomicUsize],
        level_size: usize,
    ) -> Self {
        unsafe {
            Self::try_from(vrange, tree, index, level_size).expect("failed to make physical memory allocator")
        }
    }

    /// creates a new physical memory allocator, and returns None if the invariants are not upheld
    ///
    /// # Safety
    /// must not have a mutable reference to tree or index alive once you start calling other allocator methods
    pub unsafe fn try_from(
        vrange: AVirtRange,
        tree: *mut [AtomicU8],
        index: *mut [AtomicUsize],
        level_size: usize,
    ) -> Option<Self> {
        if Self::required_tree_index_size(vrange, level_size)? == (tree.len(), index.len()) {
            // this is needed because Atomics do not have clone
            // might be slow, but shouldn't matter because this is done once
            unsafe {
                // need to do it with raw integers because this is much faster
                let tree_u8 = slice::from_raw_parts_mut(tree.as_mut_ptr() as *mut u8, tree.len());
                // do not need to initilized index to 0
                //let index_usize = slice::from_raw_parts_mut(index.as_mut_ptr() as *mut usize, index.len());
                tree_u8.fill(0);
                //index_usize.fill(0);
            }

            Some(PmemAllocator {
                addr_range: vrange,
                tree: tree as *const [AtomicU8],
                index: index as *const [AtomicUsize],
                depth: log2(vrange.size() / level_size),
                max_size: vrange.size(),
                level_size,
                free_space: AtomicUsize::new(vrange.size()),
            })
        } else {
            None
        }
    }

    /// Returns the allocation level needed for the requested allocation size, or none if `size` it is too big
    fn get_level_for_allocation_size(&self, size: usize) -> Option<usize> {
        if size > self.max_size {
            None
        } else {
            Some(min(log2(self.max_size / size), self.depth))
        }
    }

    /// Returns the level of an existing allocation
    fn get_node_from_allocation(&self, allocation: Allocation) -> TreeNode {
        let level = log2(self.max_size / allocation.size());
        assert!(level <= self.depth);
        assert!(self.addr_range.full_contains_range(&allocation.as_vrange()));

        let level_start = (1 << level) - 1;

        let addr_offset = allocation.as_usize() - self.start_addr();

        self.get_tree_node(level_start + (addr_offset / allocation.size()))
    }

    /// Returns allocated pages at least `size` bytes large, or `None` on failure
    pub fn alloc(&self, size: usize) -> Option<Allocation> {
        let level = self.get_level_for_allocation_size(size)?;

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
                    self.free_space.fetch_sub(node.size(), Ordering::AcqRel);
                    return Some(Allocation::new(node.addr(), node.size()));
                }
            }

            i += 1;
        }

        None
    }

    // returns Some(node) on failure, where node is the TreeNode that caused a problem
    fn try_alloc<'a>(&'a self, node: TreeNode<'a>) -> Option<TreeNode<'a>> {
        if node
            .data()
            .compare_exchange(0, TreeStatus::BUSY.bits(), Ordering::AcqRel, Ordering::Relaxed)
            .is_err() {
            return Some(node);
        }

        let mut current = node;

        loop {
            let child = current;

            current = current.parent()?;

            let child_type = child.child_type();

            let res = current
                .data()
                .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
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

    /// Attempts to grow or shrink the given allocation to the new size without moving the start of the allocation
    /// 
    /// If the allocation cannot be resized, `None` is returned, and the original allocation will still be valid
    // TODO: we might be able to have a function that can grow a node even if it is not aligned at the start of its new level size
    // this would require growing the allocation in both directions, but this is hard to work with page mapping so for now just return None on that case
    pub unsafe fn realloc_in_place(&self, allocation: Allocation, new_size: usize) -> Option<Allocation> {
        let old_node = self.get_node_from_allocation(allocation);
        let old_level = old_node.level();
        let new_level = self.get_level_for_allocation_size(new_size)?;

        if old_level > new_level {
            // allocation needs to be grown

            // panic safety: we know old_node has a parent since there is a level larger than it
            let mut current_node = old_node.parent().unwrap();

            loop {
                let result = current_node.data()
                    .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                        let flags = unsafe { TreeStatus::from_bits_unchecked(n) };

                        if flags.contains(TreeStatus::OCCUPY_RIGHT) {
                            None
                        } else {
                            Some(TreeStatus::BUSY.bits())
                        }
                    });
                
                if result.is_err() {
                    // this is the last node that was succesfully allocated while growing node
                    let previous_node = current_node.left();
                    unsafe { self.shrink_node(previous_node, old_level) };
                    return None;
                }

                let Some(new_node) = current_node.parent() else { break };
                if new_node.level() > new_level {
                    break;
                }

                current_node = new_node;
            }

            let new_node = current_node;

            // at this point allocation has succeeded, we just need to clear the bits of all old allocated nodes
            loop {
                current_node = current_node.left();
                if current_node.level() > old_level {
                    // we have finished clearing all old nodes bits
                    break;
                }

                current_node.data().store(0, Ordering::Release);
            }

            Some(Allocation::new(new_node.addr(), new_node.size()))
        } else if old_level < new_level {
            // allocation needs to be shrunk
            let new_node = unsafe { self.shrink_node(old_node, new_level) };
            
            Some(Allocation::new(new_node.addr(), new_node.size()))
        } else {
            // allocation can stay the same size
            Some(allocation)
        }
    }

    /// Shrinks the given node to the given level, and returns the new node
    /// 
    /// # Panics
    /// panics if level is too large
    unsafe fn shrink_node<'a>(&'a self, node: TreeNode<'a>, new_level: usize) -> TreeNode<'a> {
        if new_level <= node.level() {
            return node;
        }

        let mut current_node = node.left();

        // set all new parent nodes to occupied left
        while current_node.level() < new_level {
            current_node.data().store(TreeStatus::OCCUPY_LEFT.bits(), Ordering::Release);
            current_node = current_node.left();
        }

        // mark the final node as occupied
        current_node.data().store(TreeStatus::BUSY.bits(), Ordering::Release);

        // finaly mark old node as occupy left
        node.data().store(TreeStatus::OCCUPY_LEFT.bits(), Ordering::Release);

        current_node
    }

    /// deallocates memory referenced by allocation
    /// panics if allocation is smaller than min level size,
    /// or if allocation does not reference a range of memory managed by this allocator
    pub unsafe fn dealloc(&self, allocation: Allocation) {
        let node = self.get_node_from_allocation(allocation);

        self.dealloc_node(node, self.get_tree_node(0));

        self.free_space.fetch_add(node.size(), Ordering::AcqRel);
    }

    pub fn addr_range(&self) -> AVirtRange {
        self.addr_range
    }

    pub fn start_addr(&self) -> usize {
        self.addr_range.as_usize()
    }

    pub fn free_space(&self) -> usize {
        self.free_space.load(Ordering::Acquire)
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

            current
                .data()
                .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                    flags = unsafe { TreeStatus::from_bits_unchecked(n) };
                    flags.set_coal(child_type);
                    Some(flags.bits())
                })
                .unwrap();

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

            let res = current
                .data()
                .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
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
        unsafe { self.tree.as_ref().unwrap() }
    }

    fn index_slice(&self) -> &[AtomicUsize] {
        unsafe { self.index.as_ref().unwrap() }
    }
}

unsafe impl Send for PmemAllocator {}
unsafe impl Sync for PmemAllocator {}
