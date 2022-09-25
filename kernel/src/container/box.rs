use core::mem::{self, MaybeUninit};
use core::ops::{Deref, DerefMut};

use crate::alloc::{OrigAllocator, OrigRef};
use crate::mem::{HeapAllocation, MemOwner};
use crate::prelude::*;

#[derive(Debug)]
pub struct Box<T> {
    data: MemOwner<T>,
    allocator: OrigRef,
}

impl<T> Box<T> {
    pub fn new(data: T, mut allocator: OrigRef) -> KResult<Self> {
        Ok(Box {
            data: MemOwner::new(data, allocator.allocator())?,
            allocator,
        })
    }

    pub fn new_uninit(allocator: OrigRef) -> KResult<Box<MaybeUninit<T>>> {
        Box::new(MaybeUninit::<T>::uninit(), allocator)
    }

    pub unsafe fn from_raw(ptr: *mut T, allocator: OrigRef) -> Self {
        Box {
            data: unsafe { MemOwner::from_raw(ptr) },
            allocator,
        }
    }

    pub fn into_raw(this: Self) -> (*mut T, OrigRef) {
        let data = unsafe { ptr::read(&this.data) };
        let allocator = unsafe { ptr::read(&this.allocator) };
        mem::forget(this);
        (data.ptr_mut(), allocator)
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

    pub fn allocator(this: &mut Self) -> &dyn OrigAllocator {
        this.allocator.allocator()
    }
}

impl<T> Deref for Box<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &*self.data
    }
}

impl<T> DerefMut for Box<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut *self.data
    }
}

impl<T> Drop for Box<T> {
    fn drop(&mut self) {
        let allocation = HeapAllocation::from_ptr(Box::ptr(self));
        unsafe {
            self.allocator.allocator().dealloc_orig(allocation);
        }
    }
}
