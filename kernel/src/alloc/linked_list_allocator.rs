use core::cell::Cell;
use core::cmp::max;

use spin::Mutex;

use super::{HeapAllocator, OrigAllocator, PaRef, PageAllocator};
use crate::container::{LinkedList, ListNode, ListNodeData};
use crate::mem::{Allocation, HeapAllocation, Layout, MemOwner, PageLayout};
use crate::prelude::*;
use crate::sync::IMutex;

const HEAP_ZONE_SIZE: usize = PAGE_SIZE * 8;
const CHUNK_SIZE: usize = 1 << log2_up_const(size_of::<Node>());
// TODO: make not use 1 extra space in some scenarios
const INITIAL_CHUNK_SIZE: usize = align_up(size_of::<HeapZone>(), CHUNK_SIZE);

#[derive(Debug, Clone, Copy)]
enum ResizeResult {
    Shrink(usize),
    Remove(usize),
    NoCapacity,
}

#[derive(Debug)]
struct Node {
    list_node_data: ListNodeData<Self>,
    size: Cell<usize>,
}

impl Node {
    unsafe fn new(addr: usize, size: usize) -> MemOwner<Self> {
        let out = Node {
            list_node_data: ListNodeData::default(),
            size: Cell::new(size),
        };

        unsafe { MemOwner::new_at_addr(out, addr) }
    }

    unsafe fn resize(&self, size: usize, align: usize) -> ResizeResult {
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

    fn merge<'a>(&'a self, node: &'a Node) -> bool {
        if self.addr() + self.size() == node.addr() {
            self.size.set(self.size() + node.size());
            //self.size.fetch_add (node.size (), Ordering::SeqCst);
            true
        } else {
            false
        }
    }

    fn size(&self) -> usize {
        self.size.get()
    }

    fn set_size(&self, size: usize) {
        self.size.set(size);
    }
}

impl ListNode for Node {
    fn list_node_data(&self) -> &ListNodeData<Self> {
        &self.list_node_data
    }
}

struct HeapZone {
    list_node_data: ListNodeData<Self>,
    mem: Allocation,
    free_space: Cell<usize>,
    list: LinkedList<Node>,
}

impl HeapZone {
    // size is aligned up to page size
    unsafe fn new(size: usize, allocator: &dyn PageAllocator) -> Option<MemOwner<Self>> {
        let layout = PageLayout::new_rounded(size, PAGE_SIZE).unwrap();
        let mem = allocator.alloc(layout)?;
        let size = mem.size();
        let ptr = mem.as_usize() as *mut HeapZone;

        let mut out = HeapZone {
            list_node_data: ListNodeData::default(),
            mem,
            free_space: Cell::new(size - INITIAL_CHUNK_SIZE),
            list: LinkedList::new(),
        };

        let node = unsafe { Node::new(mem.as_usize() + INITIAL_CHUNK_SIZE, size - INITIAL_CHUNK_SIZE) };
        out.list.push(node);

        unsafe {
            ptr.write(out);
            Some(MemOwner::from_raw(ptr))
        }
    }

    fn free_space(&self) -> usize {
        self.free_space.get()
    }

    fn empty(&self) -> bool {
        self.free_space() == 0
    }

    fn contains(&self, addr: usize, size: usize) -> bool {
        (addr >= self.addr() + CHUNK_SIZE) && (addr + size <= self.addr() + CHUNK_SIZE + self.mem.size())
    }

    unsafe fn delete(&mut self, allocator: &dyn PageAllocator) {
        unsafe {
            allocator.dealloc(self.mem);
        }
    }

    unsafe fn alloc(&mut self, layout: Layout) -> Option<HeapAllocation> {
        let size = layout.size();
        let align = layout.align();

        if size > self.free_space() {
            return None;
        }

        let mut out = None;
        // to get around borrow checker
        // node that may need to be removed
        let mut rnode = None;

        for free_zone in self.list.iter() {
            let old_size = free_zone.size();
            if old_size >= size {
                match unsafe { free_zone.resize(size, align) } {
                    ResizeResult::Shrink(addr) => {
                        let alloc_size = old_size - free_zone.size();
                        let free_space = self.free_space();
                        self.free_space.set(free_space - alloc_size);
                        out = Some(HeapAllocation::new(addr, alloc_size, align));
                        break;
                    },
                    ResizeResult::Remove(addr) => {
                        rnode = Some(free_zone as *const Node);
                        self.free_space.set(self.free_space() - old_size);
                        out = Some(HeapAllocation::new(addr, old_size, align));
                        break;
                    },
                    ResizeResult::NoCapacity => continue,
                }
            }
        }

        if let Some(node) = rnode {
            // FIXME: find a way to fix ownership issue without doing this
            unsafe {
                self.list.remove_node(node.as_ref().unwrap());
            }
        }

        out
    }

    // does not chack if ptr is in this zone
    // ptr should be chuk_size aligned
    unsafe fn dealloc(&mut self, allocation: HeapAllocation) {
        let addr = allocation.addr();
        let size = allocation.size();

        let cnode = unsafe { Node::new(addr, size) };
        let (pnode, nnode) = self.get_prev_next_node(addr);

        // TODO: make less ugly
        let pnode = pnode.map(|node| unsafe { unbound(node) });
        let nnode = nnode.map(|node| unsafe { unbound(node) });

        let cnode = if let Some(pnode) = pnode {
            if pnode.merge(&cnode) {
                // cnode was never in list, no need to remove
                // TODO: probably unbound
                pnode
            } else {
                self.list.insert_after(cnode, pnode)
            }
        } else {
            self.list.push_front(cnode)
        };

        if let Some(nnode) = nnode {
            if cnode.merge(nnode) {
                self.list.remove_node(nnode);
            }
        }

        self.free_space.set(self.free_space() + size);
    }

    fn get_prev_next_node(&self, addr: usize) -> (Option<&Node>, Option<&Node>) {
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

    // TODO: add reporting of memory that is still allocated
    // safety: cannot use this heap zone after calling this method
    unsafe fn dealloc_all(&mut self, allocator: &dyn PageAllocator) {
        //assert_eq!(self.free_space.get(), self.mem.size());
        unsafe {
            allocator.dealloc(self.mem);
        }
    }
}

impl ListNode for HeapZone {
    fn list_node_data(&self) -> &ListNodeData<Self> {
        &self.list_node_data
    }
}

// TODO: add drop implementation that frees all page allocations
// This is public because it is used by CapAllocator
pub struct LinkedListAllocatorInner {
    list: LinkedList<HeapZone>,
}

impl LinkedListAllocatorInner {
    pub fn new() -> Self {
        LinkedListAllocatorInner {
            list: LinkedList::new(),
        }
    }

    pub fn alloc(&mut self, layout: Layout, page_allocator: &dyn PageAllocator) -> Option<HeapAllocation> {
        let size = layout.size();
        let align = layout.align();

        for z in self.list.iter_mut() {
            if z.free_space() >= size {
                if let Some(allocation) = unsafe { z.alloc(layout) } {
                    return Some(allocation);
                }
            }
        }

        // allocate new heapzone because there was no space in any others
        let size_inc = max(HEAP_ZONE_SIZE, size + max(align, CHUNK_SIZE) + INITIAL_CHUNK_SIZE);
        let zone = match unsafe { HeapZone::new(size_inc, page_allocator) } {
            Some(n) => n,
            None => return None,
        };

        let zone = self.list.push(zone);

        // shouldn't fail now
        unsafe { zone.alloc(layout) }
    }

    pub unsafe fn dealloc(&mut self, allocation: HeapAllocation) {
        let addr = allocation.addr();
        let size = allocation.size();

        for z in self.list.iter_mut() {
            if z.contains(addr, size) {
                unsafe {
                    z.dealloc(allocation);
                }
                return;
            }
        }

        panic!("invalid allocation passed to dealloc");
    }

    /// Deallocates all allocations in the linked list allocator
    pub unsafe fn dealloc_all(&mut self, page_allocator: &dyn PageAllocator) {
        for zone in self.list.iter_mut() {
            // safety: these zones can never be referenced after this point
            unsafe {
                zone.dealloc_all(page_allocator);
            }
        }
    }

    /// Used for implementing the OrigAllocator trait on anything that wraps a LinkedListAllocatorInner
    pub fn compute_alloc_properties(allocation: HeapAllocation) -> Option<HeapAllocation> {
        if align_of(allocation.addr()) < CHUNK_SIZE {
            None
        } else {
            let align = allocation.align();
            let size = align_up(allocation.size(), max(CHUNK_SIZE, align));
            Some(HeapAllocation::new(allocation.addr(), size, align))
        }
    }
}

// NOTE: can switch to schedular mutex once implemented
pub struct LinkedListAllocator {
    inner: IMutex<LinkedListAllocatorInner>,
    allocator: Mutex<PaRef>,
}

impl LinkedListAllocator {
    pub fn new(page_allocator: PaRef) -> Self {
        LinkedListAllocator {
            inner: IMutex::new(LinkedListAllocatorInner::new()),
            allocator: Mutex::new(page_allocator),
        }
    }
}

// TODO: add specialized realloc method
impl HeapAllocator for LinkedListAllocator {
    fn alloc(&self, layout: Layout) -> Option<HeapAllocation> {
        self.inner.lock().alloc(layout, self.allocator.lock().allocator())
    }

    unsafe fn dealloc(&self, allocation: HeapAllocation) {
        unsafe { self.inner.lock().dealloc(allocation) }
    }
}

impl OrigAllocator for LinkedListAllocator {
    fn as_heap_allocator(&self) -> &dyn HeapAllocator {
        self
    }

    fn compute_alloc_properties(&self, allocation: HeapAllocation) -> Option<HeapAllocation> {
        LinkedListAllocatorInner::compute_alloc_properties(allocation)
    }
}

impl Drop for LinkedListAllocator {
    fn drop(&mut self) {
        unsafe {
            self.inner.lock().dealloc_all(self.allocator.lock().allocator());
        }
    }
}
