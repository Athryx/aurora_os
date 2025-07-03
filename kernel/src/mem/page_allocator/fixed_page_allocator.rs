use core::sync::atomic::{AtomicBool, Ordering};

use super::PageAllocator;
use super::{PageAllocation, PageLayout};
use crate::prelude::*;

/// A page allocator that allocates just 1 big zone, used in initilization to supply page to a temporary heap allocator
pub struct FixedPageAllocator {
    mem: PageAllocation,
    alloced: AtomicBool,
}

impl FixedPageAllocator {
    pub unsafe fn new(mem: AVirtRange) -> Self {
        FixedPageAllocator {
            mem: PageAllocation::new(mem.as_usize(), mem.size()),
            alloced: AtomicBool::new(false),
        }
    }
}

unsafe impl PageAllocator for FixedPageAllocator {
    fn alloc(&self, layout: PageLayout) -> Option<PageAllocation> {
        if layout.size() > self.mem.size()
            || layout.align() > align_of_addr(self.mem.as_usize())
            || self.alloced.compare_exchange(false, true, Ordering::Relaxed, Ordering::Relaxed).is_err()
        {
            None
        } else {
            Some(self.mem)
        }
    }

    unsafe fn dealloc(&self, allocation: PageAllocation) {
        if allocation.addr() == self.mem.addr() && allocation.size() == self.mem.size() {
            self.alloced.store(false, Ordering::Relaxed);
        }
    }
}
