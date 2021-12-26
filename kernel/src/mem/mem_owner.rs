use core::alloc::Layout;
use core::ops::{Deref, DerefMut};

use crate::prelude::*;
use crate::alloc::HeapAllocator;
use super::HeapAllocation;

#[derive(Debug)]
pub struct MemOwner<T>(*mut T);

impl<T> MemOwner<T> {
	pub fn new(data: T, allocator: &dyn HeapAllocator) -> Self {
		let layout = Layout::new::<T>();

		let mut mem = allocator.alloc(layout).expect("out of memory for MemOwner");
		let ptr: *mut T = mem.as_mut_ptr();

		unsafe {
			core::ptr::write(ptr, data);
			Self::from_raw(ptr)
		}
	}

	pub unsafe fn from_raw(ptr: *mut T) -> Self {
		MemOwner(ptr)
	}

	pub unsafe fn clone(&self) -> Self {
		MemOwner(self.0)
	}

	pub fn ptr(&self) -> *const T {
		self.0 as *const T
	}

	pub fn ptr_mut(&self) -> *mut T {
		self.0
	}

	pub unsafe fn dealloc(self, allocator: &dyn HeapAllocator) {
		allocator.dealloc(HeapAllocation::from_ptr(self.0));
	}
}

impl<T> Deref for MemOwner<T> {
	type Target = T;

	fn deref(&self) -> &Self::Target {
		unsafe { self.0.as_ref().unwrap() }
	}
}

impl<T> DerefMut for MemOwner<T> {
	fn deref_mut(&mut self) -> &mut Self::Target {
		unsafe { self.0.as_mut().unwrap() }
	}
}

unsafe impl<T: Send> Send for MemOwner<T> {}
unsafe impl<T: Sync> Sync for MemOwner<T> {}
