use core::ops::{Deref, DerefMut};
use core::mem::{self, MaybeUninit};

use crate::prelude::*;
use crate::mem::{HeapAllocation, MemOwner};
use crate::alloc::{OrigRef, OrigAllocator};

#[derive(Debug)]
pub struct Box<T> {
	data: MemOwner<T>,
	allocator: OrigRef,
}

impl<T> Box<T> {
	pub fn new(data: T, allocator: OrigRef) -> KResult<Self> {
		Ok(Box {
			data: MemOwner::new(data, &*allocator)?,
			allocator,
		})
	}

	pub fn new_uninit(allocator: OrigRef) -> KResult<Box<MaybeUninit<T>>> {
		Box::new(MaybeUninit::<T>::uninit(), allocator)
	}

	pub unsafe fn from_raw(ptr: *mut T, allocator: OrigRef) -> Self {
		Box {
			data: MemOwner::from_raw(ptr),
			allocator,
		}
	}

	pub fn into_raw(self) -> (*mut T, OrigRef) {
		let data = unsafe { ptr::read(&self.data) };
		let allocator = unsafe { ptr::read(&self.allocator) };
		mem::forget(self);
		(data.ptr_mut(), allocator)
	}

	fn try_clone(&self) -> KResult<Self>
		where T: Clone {
		let (ptr, allocator) = Self::new_uninit(self.allocator.clone())?.into_raw();
		unsafe {
			ptr::write(ptr as *mut T, (**self).clone());
			Ok(Self::from_raw(ptr as *mut T, allocator))
		}
	}

	pub fn ptr(&self) -> *const T {
		self.data.ptr()
	}

	pub fn ptr_mut(&self) -> *mut T {
		self.data.ptr_mut()
	}

	pub fn allocator(&self) -> &dyn OrigAllocator {
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
