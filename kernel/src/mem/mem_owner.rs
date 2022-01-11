use core::alloc::Layout;
use core::ops::{Deref, DerefMut};

use crate::prelude::*;
use crate::alloc::OrigAllocator;
use super::HeapAllocation;

#[derive(Debug)]
pub struct MemOwner<T>(*mut T);

impl<T> MemOwner<T> {
	pub fn new(data: T, allocator: &dyn OrigAllocator) -> KResult<Self> {
		let layout = Layout::new::<T>();

		let mut mem = allocator.alloc(layout).ok_or(SysErr::OutOfMem)?;
		let ptr: *mut T = mem.as_mut_ptr();

		unsafe {
			core::ptr::write(ptr, data);
			Ok(Self::from_raw(ptr))
		}
	}

	pub unsafe fn new_at_addr(data: T, addr: usize) -> Self {
		let ptr = addr as *mut T;
		unsafe {
			ptr.write(data);
		}
		MemOwner(ptr)
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

	pub fn leak<'a>(mut self) -> &'a mut T {
		// Safety: this should point to valid data, which we are not deallocating
		unsafe {
			unbound_mut(&mut *self)
		}
	}

	pub unsafe fn dealloc(self, allocator: &dyn OrigAllocator) {
		unsafe {
			allocator.dealloc_orig(HeapAllocation::from_ptr(self.0));
		}
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
