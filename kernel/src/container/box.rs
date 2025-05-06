use core::mem::{self, MaybeUninit};
use core::ops::{Deref, DerefMut};

use crate::alloc::HeapRef;
use crate::mem::{MemOwner, MemOwnerKernelExt};
use crate::prelude::*;

#[derive(Debug)]
pub struct Box<T> {
    data: MemOwner<T>,
    allocator: HeapRef,
}

impl<T> Box<T> {
    pub fn new(data: T, mut allocator: HeapRef) -> KResult<Self> {
        Ok(Box {
            data: MemOwner::new(data, &mut allocator)?,
            allocator,
        })
    }

    pub fn new_uninit(allocator: HeapRef) -> KResult<Box<MaybeUninit<T>>> {
        Box::new(MaybeUninit::<T>::uninit(), allocator)
    }

    pub unsafe fn from_raw(ptr: *mut T, allocator: HeapRef) -> Self {
        Box {
            data: unsafe { MemOwner::from_raw(ptr) },
            allocator,
        }
    }

    pub unsafe fn from_mem_owner(mem_owner: MemOwner<T>, allocator: HeapRef) -> Self {
        Box {
            data: mem_owner,
            allocator,
        }
    }

    pub fn into_pieces(this: Self) -> (MemOwner<T>, HeapRef) {
        let data = unsafe { ptr::read(&this.data) };
        let allocator = unsafe { ptr::read(&this.allocator) };
        mem::forget(this);
        (data, allocator)
    }

    pub fn into_raw(this: Self) -> (*mut T, HeapRef) {
        let (data, allocator) = Self::into_pieces(this);
        (data.ptr_mut(), allocator)
    }

    pub fn into_mem_owner(this: Self) -> MemOwner<T> {
        let (data, _) = Self::into_pieces(this);
        data
    }

    fn try_clone(this: &Self) -> KResult<Self>
    where
        T: Clone,
    {
        let (ptr, allocator) = Box::into_raw(Self::new_uninit(this.allocator.clone())?);
        unsafe {
            ptr::write(ptr as *mut T, (**this).clone());
            Ok(Self::from_raw(ptr as *mut T, allocator))
        }
    }

    pub fn ptr(this: &Self) -> *const T {
        this.data.ptr()
    }

    pub fn ptr_mut(this: &Self) -> *mut T {
        this.data.ptr_mut()
    }

    pub fn allocator(this: &mut Self) -> &mut HeapRef {
        &mut this.allocator
    }
}

impl<T> Deref for Box<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.data
    }
}

impl<T> DerefMut for Box<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.data
    }
}

impl<T> Drop for Box<T> {
    fn drop(&mut self) {
        unsafe {
            // safety: we read out of data to copy the memowner,
            // but then never use the original mem owner
            // so it is ok to drop the new mem owner in place
            let inner = ptr::read(&self.data);
            inner.drop_in_place(&mut self.allocator);
        }
    }
}
