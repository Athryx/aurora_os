//! Provides memory allocation for userspace and rust alloc crate
// FIXME: this memory allocator is shit
// for now it is a copy paste of old kernel allocator, which is bad and splits up heap into lots of zones,
// which is not good for userspace trying to minimize amount of syscalls made

use core::cell::Cell;
use core::cmp::max;
use core::ptr::{NonNull, null_mut};
use core::mem::size_of;
use core::alloc::Layout;
use alloc::alloc::GlobalAlloc;

use bit_utils::{PAGE_SIZE, log2_up_const, align_up, align_down, align_of, Size, MemOwner};
use bit_utils::container::{LinkedList, ListNode, ListNodeData, CursorMut};

use crate::addr_space;
use crate::addr_space_manager::MapMemoryArgs;
use crate::sync::Mutex;

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

    fn list_node_data_mut(&mut self) -> &mut ListNodeData<Self> {
        &mut self.list_node_data
    }
}

struct HeapZone {
    list_node_data: ListNodeData<Self>,
    // total size of this heapzone
    size: usize,
    free_space: Cell<usize>,
    list: LinkedList<Node>,
}

impl HeapZone {
    // size is aligned up to page size
    unsafe fn new(size: usize) -> Option<MemOwner<Self>> {
        let map_result = addr_space()
            .map_memory(MapMemoryArgs {
                size: Some(Size::from_bytes(HEAP_ZONE_SIZE)),
                ..Default::default()
            }).ok()?;

        let size = map_result.size.bytes();
        let ptr = map_result.address as *mut HeapZone;

        let mut out = HeapZone {
            list_node_data: ListNodeData::default(),
            size,
            free_space: Cell::new(size - INITIAL_CHUNK_SIZE),
            list: LinkedList::new(),
        };

        let node = unsafe { Node::new(map_result.address + INITIAL_CHUNK_SIZE, size - INITIAL_CHUNK_SIZE) };
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
        (addr >= self.addr() + CHUNK_SIZE) && (addr + size <= self.addr() + CHUNK_SIZE + self.size)
    }

    unsafe fn alloc(&mut self, layout: Layout) -> Option<NonNull<[u8]>> {
        let size = layout.size();
        let align = layout.align();

        if size > self.free_space() {
            return None;
        }

        let mut cursor = self.list.cursor_start_mut();

        while let Some(free_zone) = cursor.move_next() {
            if free_zone.size() >= size {
                let old_size = free_zone.size();

                match unsafe { free_zone.resize(size, align) } {
                    ResizeResult::Shrink(addr) => {
                        let alloc_size = old_size - free_zone.size();
                        self.free_space.set(self.free_space() - alloc_size);

                        return Some(
                            NonNull::slice_from_raw_parts(
                                NonNull::new(addr as *mut u8).unwrap(),
                                alloc_size,
                            ),
                        );
                    },
                    ResizeResult::Remove(addr) => {
                        cursor.remove_prev();

                        self.free_space.set(self.free_space() - old_size);

                        return Some(
                            NonNull::slice_from_raw_parts(
                                NonNull::new(addr as *mut u8).unwrap(),
                                old_size,
                            ),
                        );
                    },
                    ResizeResult::NoCapacity => (),
                }
            }
        }

        None
    }

    // does not check if allocation is in this zone
    unsafe fn dealloc(&mut self, allocation: NonNull<[u8]>) {
        let addr = allocation.as_mut_ptr() as usize;
        let size = allocation.len();

        let new_node = unsafe { Node::new(addr, size) };
        let mut cursor = self.get_prev_next_node(addr);

        if let Some(prev_node) = cursor.prev() && prev_node.merge(&new_node) {
            // nodes were merged, do nothing
        } else {
            // only insert if nodes could not merge,
            // otherwise the new_node merged with prev_node and can now be ignored
            cursor.insert_prev(new_node);
        }

        if let Some(next_node) = cursor.next() {
            // panic safety: prev node was guarenteed ot be inserted in above code
            let prev_node = cursor.prev().unwrap();

            if prev_node.merge(next_node) {
                cursor.remove_next();
            }
        }

        self.free_space.set(self.free_space() + size);
    }

    /// Returns a cursor that points between the previous and next node for the given address
    fn get_prev_next_node(&mut self, addr: usize) -> CursorMut<Node> {
        let mut cursor = self.list.cursor_start_mut();

        loop {
            if let Some(next_node) = cursor.next() {
                // this will be the first node that had an address greatur than the desired addr,
                // so it should be the next node
                if next_node.addr() > addr {
                    return cursor;
                }
            } else {
                return cursor;
            }

            cursor.move_next();
        }
    }

    // TODO: add reporting of memory that is still allocated
    // safety: cannot use this heap zone after calling this method
    unsafe fn dealloc_all(&mut self) {
        //assert_eq!(self.free_space.get(), self.mem.size());
        unsafe {
            addr_space().unmap_memory(self as *mut _ as usize)
                .expect("failed to dealloc heap zone");
        }
    }
}

impl ListNode for HeapZone {
    fn list_node_data(&self) -> &ListNodeData<Self> {
        &self.list_node_data
    }

    fn list_node_data_mut(&mut self) -> &mut ListNodeData<Self> {
        &mut self.list_node_data
    }
}

// TODO: add drop implementation that frees all page allocations
struct LinkedListAllocatorInner {
    list: LinkedList<HeapZone>,
}

impl LinkedListAllocatorInner {
    pub const fn new() -> Self {
        LinkedListAllocatorInner {
            list: LinkedList::new(),
        }
    }

    pub fn alloc(&mut self, layout: Layout) -> Option<NonNull<[u8]>> {
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
        let zone = match unsafe { HeapZone::new(size_inc) } {
            Some(n) => n,
            None => return None,
        };

        let zone = self.list.push(zone);

        // shouldn't fail now
        unsafe { zone.alloc(layout) }
    }

    // TODO: free heap zones that are no longer in use
    pub unsafe fn dealloc(&mut self, allocation_start: NonNull<u8>, layout: Layout) {
        let allocation = LinkedListAllocator::get_allocation(allocation_start, layout)
            .expect("invalid deallocation");

        let addr = allocation.as_mut_ptr() as usize;
        let size = allocation.len();

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
    pub unsafe fn dealloc_all(&mut self) {
        for zone in self.list.iter_mut() {
            // safety: these zones can never be referenced after this point
            unsafe {
                zone.dealloc_all();
            }
        }
    }
}

pub struct LinkedListAllocator {
    inner: Mutex<LinkedListAllocatorInner>,
}

impl LinkedListAllocator {
    pub const fn new() -> Self {
        LinkedListAllocator {
            inner: Mutex::new(LinkedListAllocatorInner::new()),
        }
    }

    /// Given the pointer and layout, computes the actual allocation slice that was returned
    pub fn get_allocation(allocation_start: NonNull<u8>, layout: Layout) -> Option<NonNull<[u8]>> {
        if align_of(allocation_start.as_ptr() as usize) < CHUNK_SIZE {
            None
        } else {
            let size = align_up(layout.size(), max(CHUNK_SIZE, layout.align()));

            Some(NonNull::slice_from_raw_parts(allocation_start, size))
        }
    }
}

// TODO: add specialized realloc method
unsafe impl GlobalAlloc for LinkedListAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        match self.inner.lock().alloc(layout) {
            Some(ptr) => ptr.as_ptr().as_mut_ptr(),
            None => null_mut(),
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let ptr = NonNull::new(ptr).expect("null pointer passed to allocator");

        unsafe { self.inner.lock().dealloc(ptr, layout) }
    }
}

impl Drop for LinkedListAllocator {
    fn drop(&mut self) {
        unsafe {
            self.inner.lock().dealloc_all();
        }
    }
}

#[global_allocator]
static ALLOCATOR: LinkedListAllocator = LinkedListAllocator::new();
