use core::ops::{Deref, DerefMut};
use core::mem;

use crate::prelude::*;
use crate::mem::{HeapAllocation, MemOwner};
use crate::alloc::{AllocRef, HeapAllocator};

#[derive(Debug)]
pub struct Box<T> {
	data: MemOwner<T>,
	allocator: AllocRef,
}

impl<T> Box<T> {
	pub fn new(data: T, allocator: AllocRef) -> Self {
		Box {
			data: MemOwner::new(data, &*allocator),
			allocator,
		}
	}

	pub unsafe fn from_raw(ptr: *mut T, allocator: AllocRef) -> Self {
		Box {
			data: MemOwner::from_raw(ptr),
			allocator,
		}
	}

	pub fn into_raw(self) -> (*mut T, AllocRef) {
		let data = unsafe { ptr::read(&self.data) };
		let allocator = unsafe { ptr::read(&self.allocator) };
		mem::forget(self);
		(data.ptr_mut(), allocator)
	}

	pub fn ptr(&self) -> *const T {
		self.data.ptr()
	}

	pub fn ptr_mut(&self) -> *mut T {
		self.data.ptr_mut()
	}

	pub fn allocator(&self) -> &dyn HeapAllocator {
		&*self.allocator
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
		let allocation = HeapAllocation::from_ptr(self.ptr());
		unsafe {
			self.allocator.dealloc(allocation);
		}
	}
}
