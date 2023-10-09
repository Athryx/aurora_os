use core::alloc::Layout;

use crate::alloc::HeapRef;
use crate::prelude::*;

pub use bit_utils::MemOwner;

pub trait MemOwnerKernelExt<T> {
    fn new(data: T, allocator: &mut HeapRef) -> KResult<Self>
        where Self: Sized;

    unsafe fn drop_in_place(self, allocator: &mut HeapRef);

    unsafe fn as_box(self, allocator: HeapRef) -> Box<T>;
}

impl<T> MemOwnerKernelExt<T> for MemOwner<T> {
    fn new(data: T, allocator: &mut HeapRef) -> KResult<Self> {
        let layout = Layout::new::<T>();

        let mem = allocator.alloc(layout).ok_or(SysErr::OutOfMem)?;
        let ptr: *mut T = mem.as_mut_ptr() as *mut T;

        unsafe {
            core::ptr::write(ptr, data);
            Ok(Self::from_raw(ptr))
        }
    }

    // safety: no other mem owner must point to this memory
    unsafe fn drop_in_place(self, allocator: &mut HeapRef) {
        unsafe {
            ptr::drop_in_place(self.ptr_nonnull().as_ptr());
            allocator.dealloc(self.ptr_nonnull().cast(), Layout::new::<T>());
        }
    }

    unsafe fn as_box(self, allocator: HeapRef) -> Box<T> {
        unsafe {
            Box::from_mem_owner(self, allocator)
        }
    }
}