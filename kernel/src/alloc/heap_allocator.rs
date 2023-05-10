use core::alloc::Layout;
use core::ptr::NonNull;
use core::fmt::{self, Debug};

use crate::prelude::*;
use crate::container::Arc;

use super::{heap, CapAllocator};
use super::linked_list_allocator::LinkedListAllocator;
use super::cap_allocator::CapAllocatorWrapper;

/// A trait that represents an object that can allocate heap memory
pub unsafe trait HeapAllocator: Send + Sync {
    fn alloc(&self, layout: Layout) -> Option<NonNull<[u8]>>;
    unsafe fn dealloc(&self, allocation: NonNull<u8>, layout: Layout);

    unsafe fn realloc(&self, allocation: NonNull<u8>, old_layout: Layout, new_layout: Layout) -> Option<NonNull<[u8]>> {
        let mut mem = self.alloc(new_layout)?;

        let mut allocation_slice = NonNull::slice_from_raw_parts(
            allocation,
            old_layout.size(),
        );

        // safety: deallocagte should be called with valid `allocation` pointer
        unsafe {
            let dest_slice = &mut mem.as_mut()[..allocation_slice.len()];
            dest_slice.copy_from_slice(allocation_slice.as_mut());
        }

        unsafe {
            self.dealloc(allocation, old_layout);
        }
        Some(mem)
    }
}

// this is in inner enum so InitAllocator cannot be constructed without unsafe
#[derive(Clone)]
enum HeapRefInner {
    MainAllocator(&'static LinkedListAllocator),
    InitAllocator(*const LinkedListAllocator),
    CapAllocator(CapAllocatorWrapper),
}

/// A reference to a page allocator that can be cheaply cloned
#[derive(Clone)]
pub struct HeapRef(HeapRefInner);

impl HeapRef {
    /// Returns a HeapRef to the main heap
    /// 
    /// # Panics
    /// Panics if the heap has not yet been initialized
    pub fn heap() -> Self {
        HeapRef(HeapRefInner::MainAllocator(heap()))
    }

    pub unsafe fn init_allocator(linked_list_allocator: *const LinkedListAllocator) -> Self {
        HeapRef(HeapRefInner::InitAllocator(linked_list_allocator))
    }

    pub fn cap_allocator(cap_allocator: CapAllocatorWrapper) -> Self {
        HeapRef(HeapRefInner::CapAllocator(cap_allocator))
    }

    pub fn from_arc(allocator: Arc<CapAllocator>) -> Self {
        HeapRef(HeapRefInner::CapAllocator(allocator.into()))
    }

    pub fn alloc(&mut self, layout: Layout) -> Option<NonNull<[u8]>> {
        match self.0 {
            HeapRefInner::MainAllocator(allocator) => allocator.alloc(layout),
            HeapRefInner::InitAllocator(init_allocator) => unsafe { (*init_allocator).alloc(layout) },
            HeapRefInner::CapAllocator(ref mut cap_allocator) => cap_allocator.heap_alloc(layout),
        }
    }

    pub unsafe fn dealloc(&mut self, allocation: NonNull<u8>, layout: Layout) {
        unsafe {
            match self.0 {
                HeapRefInner::MainAllocator(allocator) => allocator.dealloc(allocation, layout),
                HeapRefInner::InitAllocator(init_allocator) => (*init_allocator).dealloc(allocation, layout),
                HeapRefInner::CapAllocator(ref mut cap_allocator) => cap_allocator.heap_dealloc(allocation, layout),
            }
        }
    }

    pub unsafe fn realloc(&mut self, allocation: NonNull<u8>, old_layout: Layout, new_layout: Layout) -> Option<NonNull<[u8]>> {
        unsafe {
            match self.0 {
                HeapRefInner::MainAllocator(allocator) => allocator.realloc(allocation, old_layout, new_layout),
                HeapRefInner::InitAllocator(init_allocator) => (*init_allocator).realloc(allocation, old_layout, new_layout),
                HeapRefInner::CapAllocator(ref mut cap_allocator) => cap_allocator.heap_realloc(allocation, old_layout, new_layout),
            }
        }
    }
}

unsafe impl Send for HeapRef {}
unsafe impl Sync for HeapRef {}

impl Debug for HeapRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "(AllocRef)")
    }
}