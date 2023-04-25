use core::alloc::Layout;
use core::ptr::NonNull;

use crate::make_alloc_ref;
use crate::prelude::*;

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

make_alloc_ref!(AllocRef, AllocRefInner, HeapAllocator);