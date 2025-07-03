use core::alloc::Layout;
use core::ptr::NonNull;

use spin::Once;

use super::heap_allocator::{LinkedListAllocator, HeapAllocator, HeapRef, heap};
use super::page_allocator::{PmemManager, PageAllocator, PaRef, zm, PageAllocation, PageLayout};
use crate::cap::{CapObject, CapType};
use crate::container::Arc;
use crate::prelude::*;
use crate::sync::{IMutex, IMutexGuard};

#[derive(Debug)]
struct TrackingAllocatorInner {
    parent: Option<Arc<TrackingAllocator>>,
    is_alive: bool,
    max_capacity: usize,
    prealloc_size: usize,
    used_size: usize,
}

impl TrackingAllocatorInner {
    /// Gets the closest alive parent, and reassignes this cap allocators parent to the closest alive parent
    fn get_parent(&mut self) -> Option<IMutexGuard<TrackingAllocatorInner>> {
        loop {
            let parent = self.parent.as_ref()?.clone();
            let parent_inner = parent.inner.lock();
            if parent_inner.is_alive {
                // transmute away lifetime of mutex guard
                // safety: this is ok because when we return this mutex, it is burrowed from the parent field of self
                // when we return, lifetime is bound to the lifetime of self
                let transmute_temp = unsafe { core::mem::transmute(parent_inner) };
                return Some(transmute_temp);
            }

            let new_parent = parent_inner.parent.clone();
            drop(parent_inner);
            self.parent = new_parent;
        }
    }

    const PREALLOC_RECURSE_DEPTH: usize = 8;

    // TODO: remove recurse depth
    // TODO: implement all extra features of prealloc
    // this is a temporary hack to stop malicous processess causing a kernel stack overflow
    // try to find a better way to avoid stack overflow without limiting prealloc depth
    fn prealloc_inner(
        &mut self,
        bytes: usize,
        recurse_depth: &mut usize,
    ) -> KResult<()> {
        *recurse_depth -= 1;
        if *recurse_depth == 0 {
            // FIXME: dont use recurse depth
            return Err(SysErr::Unknown);
        }

        if self.used_size + self.prealloc_size + bytes > self.max_capacity {
            return Err(SysErr::OutOfMem);
        }

        // If this is the root node (no parent), return OutOfMem, because we can never prealloc, so we are out of memory
        let mut parent_inner = self.get_parent()
            .ok_or(SysErr::OutOfMem)?;

        // if parent doesn't have enough prealloced memory for us to take, ask them to prealloc
        if bytes > parent_inner.prealloc_size {
            let prealloc_size = align_up(bytes - parent_inner.prealloc_size, PAGE_SIZE);
            Self::prealloc_inner(&mut parent_inner, prealloc_size, recurse_depth)?;
        }

        parent_inner.prealloc_size -= bytes;
        parent_inner.used_size += bytes;
        drop(parent_inner);
        self.prealloc_size += bytes;

        Ok(())
    }

    /// Mark bytes as allocated from the allocator, returns out of mem on failure
    pub fn alloc_bytes(&mut self, bytes: usize) -> KResult<()> {
        if bytes > self.prealloc_size {
            let prealloc_size = align_up(bytes - self.prealloc_size, PAGE_SIZE);
            let mut recurse_depth = Self::PREALLOC_RECURSE_DEPTH;

            self.prealloc_inner(prealloc_size, &mut recurse_depth)?;
        }

        self.prealloc_size -= bytes;
        self.used_size += bytes;

        Ok(())
    }

    /// Marks bytes as dealloced in this allocator
    pub fn dealloc_bytes(&mut self, bytes: usize) {
        assert!(
            self.used_size >= bytes,
            "tried to free to many bytes from this allocator"
        );
        self.prealloc_size += bytes;
        self.used_size -= bytes;
    }
}

/// an allocator that makes up the allocator tree that the kernel presents in its api to the userspace
#[derive(Debug)]
pub struct TrackingAllocator {
    inner: IMutex<TrackingAllocatorInner>,
}

impl TrackingAllocator {
    pub fn new_root(total_pages: usize) -> Self {
        Self {
            inner: IMutex::new(TrackingAllocatorInner {
                parent: None,
                is_alive: true,
                max_capacity: PAGE_SIZE * total_pages,
                prealloc_size: PAGE_SIZE * total_pages,
                used_size: 0,
            }),
        }
    }

    /// Marks the allocator as dead
    pub fn kill_allocator(&self) {
        self.inner.lock().is_alive = false;
    }
}

impl CapObject for TrackingAllocator {
    const TYPE: CapType = CapType::Allocator;
}

/// References a [`TrackingAllocator`] and implements page and heap allocation traits
#[derive(Debug, Clone)]
pub struct TrackingAllocatorWrapper {
    allocator: Arc<TrackingAllocator>,
}

impl From<Arc<TrackingAllocator>> for TrackingAllocatorWrapper {
    fn from(allocator: Arc<TrackingAllocator>) -> Self {
        TrackingAllocatorWrapper {
            allocator
        }
    }
}

impl TrackingAllocatorWrapper {
    /// Gets the closest alive parent and returns a lock to its inner data
    fn with_inner<T>(&mut self, f: impl FnOnce(&mut TrackingAllocatorInner) -> T) -> T {
        let mut allocator = self.allocator.inner.lock();
        if allocator.is_alive {
            f(&mut allocator)
        } else {
            let mut parent_inner = allocator.get_parent()
                .expect("root allocator died");
            let out = f(&mut parent_inner);
            drop(parent_inner);

            let new_allocator = allocator.parent.clone().unwrap();
            drop(allocator);
            self.allocator = new_allocator;
            out
        }
    }

    fn alloc_bytes(&mut self, size: usize) -> KResult<()> {
        self.with_inner(|inner| inner.alloc_bytes(size))
    }

    fn dealloc_bytes(&mut self, size: usize) {
        self.with_inner(|inner| inner.dealloc_bytes(size))
    }

    pub fn page_alloc(&mut self, layout: PageLayout) -> Option<PageAllocation> {
        let alloc_size = PmemManager::get_allocation_size_for_layout(layout);
        self.alloc_bytes(alloc_size).ok()?;

        let allocation = zm().alloc(layout);

        if allocation.is_none() {
            self.dealloc_bytes(alloc_size);
            None
        } else {
            allocation
        }
    }

    pub unsafe fn page_dealloc(&mut self, allocation: PageAllocation) {
        self.dealloc_bytes(allocation.size());

        unsafe {
            zm().dealloc(allocation);
        }
    }

    unsafe fn page_realloc_inner(&mut self, allocation: PageAllocation, layout: PageLayout, in_place: bool) -> Option<PageAllocation> {
        let old_size = allocation.size();
        let new_size = PmemManager::get_allocation_size_for_layout(layout);
        if old_size == new_size {
            return Some(allocation);
        }

        // only realloc if we are growing memory
        // otherwise if allocating from memory allocator fails, but we already shrunk zone,
        // regrowing the bytes could fail
        if new_size > old_size {
            self.alloc_bytes(new_size - old_size).ok()?;
        }

        let new_allocation = unsafe {
            if in_place {
                zm().realloc_in_place(allocation, layout)
            } else {
                zm().realloc(allocation, layout)
            }
        };

        if new_allocation.is_none() {
            if new_size > old_size {
                self.dealloc_bytes(new_size - old_size);
            }
            None
        } else {
            if old_size > new_size {
                self.dealloc_bytes(old_size - new_size);
            }
            new_allocation
        }
    }

    pub unsafe fn page_realloc(&mut self, allocation: PageAllocation, layout: PageLayout) -> Option<PageAllocation> {
        unsafe {
            self.page_realloc_inner(allocation, layout, false)
        }
    }

    pub unsafe fn page_realloc_in_place(&mut self, allocation: PageAllocation, layout: PageLayout) -> Option<PageAllocation> {
        unsafe {
            self.page_realloc_inner(allocation, layout, true)
        }
    }

    pub fn heap_alloc(&mut self, layout: Layout) -> Option<NonNull<[u8]>> {
        let allocation = heap().alloc(layout)?;

        let result = self.alloc_bytes(allocation.len());

        if result.is_err() {
            unsafe {
                heap().dealloc(allocation.as_non_null_ptr(), layout);
            }
            None
        } else {
            Some(allocation)
        }
    }

    pub unsafe fn heap_dealloc(&mut self, allocation_start: NonNull<u8>, layout: Layout) {
        let allocation = LinkedListAllocator::get_allocation(allocation_start, layout)
            .expect("invalid deallocation");

        self.dealloc_bytes(allocation.len());
        unsafe { heap().dealloc(allocation_start, layout) }
    }

    pub unsafe fn heap_realloc(&mut self, allocation: NonNull<u8>, old_layout: Layout, new_layout: Layout) -> Option<NonNull<[u8]>> {
        let old_allocation = LinkedListAllocator::get_allocation(allocation, old_layout)
            .expect("invalid reallocation");
        let old_size = old_allocation.len();

        let new_size = LinkedListAllocator::get_allocation(allocation, new_layout)
            .expect("invalid reallocation")
            .len();

        if old_size == new_size {
            return Some(old_allocation);
        }

        // only realloc if we are growing memory
        // otherwise if allocating from memory allocator fails, but we already shrunk zone,
        // regrowing the bytes could fail
        if new_size > old_size {
            self.alloc_bytes(new_size - old_size).ok()?;
        }

        let new_allocation = unsafe {
            heap().realloc(allocation, old_layout, new_layout)
        };

        if new_allocation.is_none() {
            if new_size > old_size {
                self.dealloc_bytes(new_size - old_size);
            }
            None
        } else {
            if old_size > new_size {
                self.dealloc_bytes(old_size - new_size);
            }
            new_allocation
        }
    }
}

pub(super) static ROOT_ALLOCATOR: Once<Arc<TrackingAllocator>> = Once::new();

/// Returns the root TrackingAllocator
/// 
/// # Panics
/// Panics if the root TrackingAllocator has not yet been intialized
pub fn root_alloc() -> &'static Arc<TrackingAllocator> {
    ROOT_ALLOCATOR
        .get()
        .expect("root allocator accessed before it was initilized")
}

/// Returns the root TrackingAllocator
/// 
/// # Panics
/// Panics if the root TrackingAllocator has not yet been intialized
pub fn root_alloc_ref() -> HeapRef {
    HeapRef::tracking_allocator(root_alloc().clone().into())
}

/// Returns the root TrackingAllocator
/// 
/// # Panics
/// Panics if the root TrackingAllocator has not yet been intialized
pub fn root_alloc_page_ref() -> PaRef {
    PaRef::tracking_allocator(root_alloc().clone().into())
}
