use core::sync::atomic::{AtomicBool, Ordering};

use super::PageAllocator;
use crate::mem::{Allocation, PageLayout};
use crate::prelude::*;

/// A page allocator that allocates just 1 big zone, used in initilization to supply page to a temporary heap allocator
pub struct FixedPageAllocator {
    mem: Allocation,
    alloced: AtomicBool,
}

impl FixedPageAllocator {
    pub unsafe fn new(mem: AVirtRange) -> Self {
        FixedPageAllocator {
            mem: Allocation::new(mem.as_usize(), mem.size()),
            alloced: AtomicBool::new(false),
        }
    }
}

unsafe impl PageAllocator for FixedPageAllocator {
    fn alloc(&self, layout: PageLayout) -> Option<Allocation> {
        if self.alloced.load(Ordering::Acquire)
            || layout.size() > self.mem.size()
            || layout.align() > align_of_addr(self.mem.as_usize())
        {
            None
        } else {
            self.alloced.store(true, Ordering::Release);
            Some(self.mem)
        }
    }

    unsafe fn dealloc(&self, allocation: Allocation) {
        if allocation.addr() == self.mem.addr() && allocation.size() == self.mem.size() {
            self.alloced.store(false, Ordering::Release);
        }
    }
}
