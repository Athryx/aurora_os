use core::mem;
use core::alloc::Layout;
use core::sync::atomic::AtomicPtr;
use core::cell::Cell;
use core::cmp::max;

use sys::PAGE_SIZE;

use crate::uses::*;
use crate::collections::LinkedList;
use crate::ptr::{UniqueMut, UniquePtr, UniqueRef};
use crate::{alloc, dealloc, impl_list_node};
use super::{Allocation, MemOwner};

const INITIAL_HEAP_SIZE: usize = PAGE_SIZE * 8;
const HEAP_INC_SIZE: usize = PAGE_SIZE * 4;
const CHUNK_SIZE: usize = 1 << log2_up_const(mem::size_of::<Node>());
// TODO: make not use 1 extra space in some scenarios
const INITIAL_CHUNK_SIZE: usize = align_up(mem::size_of::<HeapZone>(), CHUNK_SIZE);

#[derive(Debug, Clone, Copy)]
enum ResizeResult
{
	Shrink(usize),
	Remove(usize),
	NoCapacity,
}

#[derive(Debug)]
struct Node
{
	prev: AtomicPtr<Node>,
	next: AtomicPtr<Node>,
	size: Cell<usize>,
}

impl Node
{
	unsafe fn new(addr: usize, size: usize) -> MemOwner<Self>
	{
		let ptr = addr as *mut Node;

		let out = Node {
			prev: AtomicPtr::new(null_mut()),
			next: AtomicPtr::new(null_mut()),
			size: Cell::new(size),
		};
		ptr.write(out);

		MemOwner::from_raw(ptr)
	}

	unsafe fn resize(&self, size: usize, align: usize) -> ResizeResult
	{
		let self_size = self.size();
		if size > self_size {
			return ResizeResult::NoCapacity;
		}

		let naddr = align_down(self.addr() + (self_size - size), max(align, CHUNK_SIZE));
		// alignment might make it less
		if naddr < self.addr() {
			return ResizeResult::NoCapacity;
		}

		let nsize = naddr - self.addr();
		if nsize >= CHUNK_SIZE {
			self.set_size(nsize);
			ResizeResult::Shrink(naddr)
		} else {
			ResizeResult::Remove(naddr)
		}
		// shouldn't need to check for case where allocation only partly covers node, since this should be impossible
	}

	fn merge<'a>(&'a self, node: &'a Node) -> bool
	{
		if self.addr() + self.size() == node.addr() {
			self.size.set(self.size() + node.size());
			//self.size.fetch_add (node.size (), Ordering::SeqCst);
			true
		} else {
			false
		}
	}

	fn size(&self) -> usize
	{
		self.size.get()
	}

	fn set_size(&self, size: usize)
	{
		self.size.set(size);
	}
}

impl_list_node!(Node, prev, next);

struct HeapZone
{
	prev: AtomicPtr<HeapZone>,
	next: AtomicPtr<HeapZone>,
	mem: Allocation,
	free_space: Cell<usize>,
	list: LinkedList<Node>,
}

impl HeapZone
{
	// size is aligned up to page size
	unsafe fn new(size: usize) -> Option<MemOwner<Self>>
	{
		let size = align_up(size, PAGE_SIZE);
		let mem = alloc(size)?;
		let size = mem.len();
		let ptr = mem.as_usize() as *mut HeapZone;

		let mut out = HeapZone {
			prev: AtomicPtr::new(null_mut()),
			next: AtomicPtr::new(null_mut()),
			mem,
			free_space: Cell::new(size - INITIAL_CHUNK_SIZE),
			list: LinkedList::new(),
		};

		let node = Node::new(
			mem.as_usize() + INITIAL_CHUNK_SIZE,
			size - INITIAL_CHUNK_SIZE,
		);
		out.list.push(node);

		ptr.write(out);

		Some(MemOwner::from_raw(ptr))
	}

	fn free_space(&self) -> usize
	{
		self.free_space.get()
	}

	fn empty(&self) -> bool
	{
		self.free_space() == 0
	}

	fn contains(&self, addr: usize, size: usize) -> bool
	{
		(addr >= self.addr() + CHUNK_SIZE)
			&& (addr + size <= self.addr() + CHUNK_SIZE + self.mem.len())
	}

	unsafe fn delete(&mut self)
	{
		dealloc(self.mem);
	}

	unsafe fn alloc(&mut self, layout: Layout) -> *mut u8
	{
		let size = layout.size();
		let align = layout.align();

		if size > self.free_space() {
			return null_mut();
		}

		let mut out = 0;
		// to get around borrow checker
		// node that may need to be removed
		let mut rnode = None;

		for free_zone in self.list.iter() {
			let old_size = free_zone.size();
			if old_size >= size {
				match free_zone.resize(size, align) {
					ResizeResult::Shrink(addr) => {
						let free_space = self.free_space();
						self.free_space
							.set(free_space - old_size + free_zone.size());
						out = addr;
						break;
					},
					ResizeResult::Remove(addr) => {
						rnode = Some(free_zone.ptr());
						self.free_space.set(self.free_space() - old_size);
						out = addr;
						break;
					},
					ResizeResult::NoCapacity => continue,
				}
			}
		}

		if let Some(node) = rnode {
			// FIXME: find a way to fix ownership issue without doing this
			self.list
				.remove_node(UniqueRef::new(node.as_ref().unwrap()));
		}

		out as *mut u8
	}

	// does not chack if ptr is in this zone
	// ptr should be chuk_size aligned
	unsafe fn dealloc(&mut self, ptr: *mut u8, layout: Layout)
	{
		let addr = ptr as usize;
		let size = align_up(layout.size(), max(CHUNK_SIZE, layout.align()));

		let cnode = Node::new(addr, size);
		let (pnode, nnode) = self.get_prev_next_node(addr);

		// TODO: make less ugly
		let pnode = pnode.map(|ptr| ptr.unbound());
		let nnode = nnode.map(|ptr| ptr.unbound());

		let cnode = if let Some(pnode) = pnode {
			if pnode.merge(&cnode) {
				// cnode was never in list, no need to remove
				UniqueMut::from_ptr(pnode.ptr() as *mut _)
			} else {
				self.list.insert_after(cnode, pnode)
			}
		} else {
			self.list.push_front(cnode)
		};

		if let Some(nnode) = nnode {
			if cnode.merge(&nnode) {
				self.list.remove_node(nnode);
			}
		}

		self.free_space.set(self.free_space() + size);
	}

	// TODO: make less dangerous
	fn get_prev_next_node(&self, addr: usize)
		-> (Option<UniqueRef<Node>>, Option<UniqueRef<Node>>)
	{
		let mut pnode = None;
		let mut nnode = None;
		for region in self.list.iter() {
			if region.addr() > addr {
				nnode = Some(region);
				break;
			}
			pnode = Some(region);
		}

		(pnode, nnode)
	}
}

impl_list_node!(HeapZone, prev, next);

pub struct LinkedListAllocator
{
	list: LinkedList<HeapZone>,
}

impl LinkedListAllocator
{
	pub fn new() -> LinkedListAllocator
	{
		let node = unsafe {
			HeapZone::new(INITIAL_HEAP_SIZE).expect("failed to allocate pages for kernel heap")
		};
		let mut list = LinkedList::new();
		list.push(node);

		LinkedListAllocator {
			list,
		}
	}

	pub unsafe fn alloc(&mut self, layout: Layout) -> *mut u8
	{
		let size = layout.size();
		let align = layout.align();

		for mut z in self.list.iter_mut() {
			if z.free_space() >= size {
				let ptr = z.alloc(layout);
				if ptr.is_null() {
					continue;
				} else {
					return ptr;
				}
			}
		}

		// allocate new heapzone because there was no space in any others
		let size_inc = max(
			HEAP_INC_SIZE,
			size + max(align, CHUNK_SIZE) + INITIAL_CHUNK_SIZE,
		);
		let zone = match HeapZone::new(size_inc) {
			Some(n) => n,
			None => return null_mut(),
		};

		let mut zone = self.list.push(zone);

		// shouldn't fail now
		zone.alloc(layout)
	}

	pub unsafe fn dealloc(&mut self, ptr: *mut u8, layout: Layout)
	{
		let addr = ptr as usize;
		assert!(align_of(addr) >= CHUNK_SIZE);
		let size = layout.size();

		for mut z in self.list.iter_mut() {
			if z.contains(addr, size) {
				z.dealloc(ptr, layout);
				return;
			}
		}

		panic!("invalid pointer passed to dealloc");
	}
}
