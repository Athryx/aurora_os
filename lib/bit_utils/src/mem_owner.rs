use core::ops::{Deref, DerefMut};
use core::ptr::NonNull;
use core::marker::PhantomData;

/// Represents an owned object in memory, but does not control allocation for that object
#[derive(Debug)]
pub struct MemOwner<T>{
    pointer: NonNull<T>,
    _marker: PhantomData<T>,
}

impl<T> MemOwner<T> {
    pub unsafe fn new_at_addr(data: T, addr: usize) -> Self {
        let ptr = addr as *mut T;
        unsafe {
            ptr.write(data);
            MemOwner::from_raw(ptr)
        }
    }

    pub unsafe fn from_raw(ptr: *mut T) -> Self {
        MemOwner {
            pointer: NonNull::new(ptr).unwrap(),
            _marker: PhantomData,
        }
    }

    pub fn ptr(&self) -> *const T {
        self.ptr_mut() as *const _
    }

    pub fn ptr_mut(&self) -> *mut T {
        self.pointer.as_ptr()
    }

    pub fn ptr_nonnull(&self) -> NonNull<T> {
        self.pointer
    }

    pub fn leak<'a>(mut self) -> &'a mut T
    where Self: 'a {
        // Safety: this should point to valid data, which we are not deallocating
        unsafe { self.pointer.as_mut() }
    }
}

impl<T> Deref for MemOwner<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { self.pointer.as_ref() }
    }
}

impl<T> DerefMut for MemOwner<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { self.pointer.as_mut() }
    }
}

unsafe impl<T: Send> Send for MemOwner<T> {}
unsafe impl<T: Sync> Sync for MemOwner<T> {}