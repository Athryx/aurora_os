use core::ptr::NonNull;
use core::marker::PhantomData;
use core::alloc::Layout;
use core::ops::{Deref, DerefMut, Index, IndexMut};
use core::slice::SliceIndex;
use core::iter::FusedIterator;
use core::fmt;

use crate::prelude::*;
use crate::alloc::{AllocRef, HeapAllocator};
use crate::mem::HeapAllocation;

struct RawVec<T> {
	ptr: NonNull<T>,
	cap: usize,
	marker: PhantomData<T>,
	allocer: AllocRef,
}

// code from rustonomicon
impl<T> RawVec<T> {
	const fn new(allocer: AllocRef) -> Self {
		// !0 is usize::MAX. This branch should be stripped at compile time.
		let cap = if size_of::<T>() == 0 { !0 } else { 0 };

		// `NonNull::dangling()` doubles as "unallocated" and "zero-sized allocation"
		RawVec {
			ptr: NonNull::dangling(),
			cap,
			marker: PhantomData,
			allocer,
		}
	}

	// tries to create a raw vec with specified capacity, returns out of mem on failure
	fn try_with_capacity(allocer: AllocRef, cap: usize) -> KResult<Self> {
		if size_of::<T>() == 0 {
			Ok(RawVec::new(allocer))
		} else {
			let layout = Layout::array::<T>(cap).unwrap();
			let ptr = allocer.alloc(layout).ok_or(SysErr::OutOfMem)?.as_mut_ptr();

			Ok(RawVec {
				ptr: NonNull::new(ptr).unwrap(),
				cap,
				marker: PhantomData,
				allocer,
			})
		}
	}

	// returns out of mem on failure
	fn try_grow(&mut self) -> KResult<()> {
		// since we set the capacity to usize::MAX when T has size 0,
		// getting to here necessarily means the Vec is overfull.
		assert!(size_of::<T>() != 0, "capacity overflow");

		let (new_cap, new_layout) = if self.cap == 0 {
			(1, Layout::array::<T>(1).unwrap())
		} else {
			// This can't overflow because we ensure self.cap <= isize::MAX.
			let new_cap = 2 * self.cap;

			// `Layout::array` checks that the number of bytes is <= usize::MAX,
			// but this is redundant since old_layout.size() <= isize::MAX,
			// so the `unwrap` should never fail.
			let new_layout = Layout::array::<T>(new_cap).unwrap();
			(new_cap, new_layout)
		};

		// Ensure that the new allocation doesn't exceed `isize::MAX` bytes.
		assert!(
			new_layout.size() <= isize::MAX as usize,
			"Allocation too large"
		);

		let new_alloc = if self.cap == 0 {
			self.allocer.alloc(new_layout)
		} else {
			let old_ptr = self.ptr.as_ptr() as *mut u8;
			let old_alloc = HeapAllocation::from_ptr(old_ptr);
			unsafe {
				self.allocer.realloc(old_alloc, new_layout)
			}
		};

		// If allocation fails, `new_ptr` will be null, in which case we abort.
		match new_alloc {
			Some(mut a) => {
				self.ptr = NonNull::new(a.as_mut_ptr()).unwrap();
				self.cap = new_cap;
				Ok(())
			},
			None => Err(SysErr::OutOfMem),
		}
	}
}

impl<T> Drop for RawVec<T> {
	fn drop(&mut self) {
		let elem_size = size_of::<T>();

		if self.cap != 0 && elem_size != 0 {
			let alloc = HeapAllocation::from_ptr(self.ptr.as_ptr());
			unsafe {
				self.allocer.dealloc(alloc);
			}
		}
	}
}

unsafe impl<T: Send> Send for RawVec<T> {}
unsafe impl<T: Sync> Sync for RawVec<T> {}

pub struct Vec<T> {
	inner: RawVec<T>,
	len: usize,
}

impl<T> Vec<T> {
	pub const fn new(allocer: AllocRef) -> Self {
		Vec {
			inner: RawVec::new(allocer),
			len: 0,
		}
	}

	pub fn try_with_capacity(allocer: AllocRef, cap: usize) -> KResult<Self> {
		Ok(Vec {
			inner: RawVec::try_with_capacity(allocer, cap)?,
			len: 0,
		})
	}

	// returns a const pointer to the object at the specified index
	unsafe fn coff(&self, index: usize) -> *const T {
		self.as_ptr().add(index)
	}

	// returns a mutable pointer to the object at the specified index
	unsafe fn off(&mut self, index: usize) -> *mut T {
		self.as_mut_ptr().add(index)
	}

	pub fn len(&self) -> usize {
		self.len
	}

	pub fn cap(&self) -> usize {
		self.inner.cap
	}

	pub fn as_ptr(&self) -> *const T {
		self.inner.ptr.as_ptr() as *const T
	}

	pub fn as_mut_ptr(&mut self) -> *mut T {
		self.inner.ptr.as_ptr()
	}

	pub fn as_slice(&self) -> &[T] {
		unsafe {
			core::slice::from_raw_parts(self.as_ptr(), self.len)
		}
	}

	pub fn as_mut_slice(&mut self) -> &mut [T] {
		unsafe {
			core::slice::from_raw_parts_mut(self.as_mut_ptr(), self.len)
		}
	}

	pub fn allocator(&self) -> &dyn HeapAllocator {
		self.inner.allocer.allocator()
	}

	pub fn push(&mut self, object: T) -> KResult<()> {
		if self.len == self.cap() {
			self.inner.try_grow()?;
		}

		unsafe {
			ptr::write(self.off(self.len), object);
		}

		self.len += 1;

		Ok(())
	}

	pub fn pop(&mut self) -> Option<T> {
		if self.len == 0 {
			None
		} else {
			self.len -= 1;

			unsafe {
				Some(ptr::read(self.off(self.len)))
			}
		}
	}

	pub fn insert(&mut self, index: usize, object: T) -> KResult<()> {
		assert!(index <= self.len, "index out of bounds");

		if self.len == self.cap() {
			self.inner.try_grow()?;
		}

		let ncpy = self.len - index;

		unsafe {
			ptr::copy(self.off(index), self.off(index + 1), ncpy);
			ptr::write(self.off(index), object);
		}

		self.len += 1;

		Ok(())
	}

	pub fn remove(&mut self, index: usize) -> T {
		assert!(index < self.len, "index out of bounds");

		let out = unsafe {
			ptr::read(self.off(index))
		};

		self.len -= 1;
		let ncpy = self.len - index;

		unsafe {
			ptr::copy(self.off(index + 1), self.off(index), ncpy);
		}

		out
	}

	pub fn iter<'a>(&'a self) -> Iter<'a, T> {
		Iter {
			inner: RawIter::new(self.as_slice()),
			marker: PhantomData,
		}
	}

	pub fn iter_mut<'a>(&'a self) -> IterMut<'a, T> {
		IterMut {
			inner: RawIter::new(self.as_slice()),
			marker: PhantomData,
		}
	}

	pub fn into_iter(self) -> IntoIter<T> {
		let slice = self.as_slice();
		// to get around borrow checker
		let buffer = unsafe {
			ptr::read(&self.inner)
		};

		IntoIter {
			inner: RawIter::new(slice),
			_buffer: buffer,
		}
	}
}

impl<T> Deref for Vec<T> {
	type Target = [T];

	fn deref(&self) -> &Self::Target {
		self.as_slice()
	}
}

impl<T> DerefMut for Vec<T> {
	fn deref_mut(&mut self) -> &mut Self::Target {
		self.as_mut_slice()
	}
}

impl<T, I: SliceIndex<[T]>> Index<I> for Vec<T> {
	type Output = I::Output;

	fn index(&self, index: I) -> &Self::Output {
		Index::index(self.as_slice(), index)
	}
}

impl<T, I: SliceIndex<[T]>> IndexMut<I> for Vec<T> {
	fn index_mut(&mut self, index: I) -> &mut Self::Output {
		IndexMut::index_mut(self.as_mut_slice(), index)
	}
}

impl<T: fmt::Debug> fmt::Debug for Vec<T> {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		fmt::Debug::fmt(self.as_slice(), f)
	}
}

impl<T> Drop for Vec<T> {
	fn drop(&mut self) {
		while let Some(_) = self.pop() {}
	}
}

struct RawIter<T> {
	// inclusive
	start: usize,
	// exlusive
	end: usize,
	marker: PhantomData<*mut T>,
}

impl<T> RawIter<T> {
	fn new(data: &[T]) -> Self {
		let addr = data.as_ptr() as usize;

		RawIter {
			start: addr,
			end: addr + Self::elem_size() * data.len(),
			marker: PhantomData,
		}
	}

	fn elem_size() -> usize {
		if size_of::<T>() == 0 {
			1
		} else {
			size_of::<T>()
		}
	}
}

impl<T> Iterator for RawIter<T> {
	type Item = *mut T;

	fn next(&mut self) -> Option<Self::Item> {
		if self.start == self.end {
			None
		} else {
			let out = self.start as *mut T;
			self.start = self.start + Self::elem_size();
			Some(out)
		}
	}

	fn size_hint(&self) -> (usize, Option<usize>) {
		let bound = (self.end - self.start) / Self::elem_size();
		(bound, Some(bound))
	}
}

impl<T> DoubleEndedIterator for RawIter<T> {
	fn next_back(&mut self) -> Option<Self::Item> {
		if self.start == self.end {
			None
		} else {
			self.start = self.start - Self::elem_size();
			Some(self.start as *mut T)
		}
	}
}

impl<T> ExactSizeIterator for RawIter<T> {}
impl<T> FusedIterator for RawIter<T> {}

pub struct Iter<'a, T: 'a> {
	inner: RawIter<T>,
	marker: PhantomData<&'a T>,
}

impl<'a, T> Iterator for Iter<'a, T> {
	type Item = &'a T;

	fn next(&mut self) -> Option<Self::Item> {
		unsafe {
			self.inner.next().map(|ptr| ptr.as_ref().unwrap())
		}
	}

	fn size_hint(&self) -> (usize, Option<usize>) {
		self.inner.size_hint()
	}
}

impl<T> DoubleEndedIterator for Iter<'_, T> {
	fn next_back(&mut self) -> Option<Self::Item> {
		unsafe {
			self.inner.next_back().map(|ptr| ptr.as_ref().unwrap())
		}
	}
}

impl<T> ExactSizeIterator for Iter<'_, T> {}
impl<T> FusedIterator for Iter<'_, T> {}

pub struct IterMut<'a, T: 'a> {
	inner: RawIter<T>,
	marker: PhantomData<&'a T>,
}

impl<'a, T> Iterator for IterMut<'a, T> {
	type Item = &'a mut T;

	fn next(&mut self) -> Option<Self::Item> {
		unsafe {
			self.inner.next().map(|ptr| ptr.as_mut().unwrap())
		}
	}

	fn size_hint(&self) -> (usize, Option<usize>) {
		self.inner.size_hint()
	}
}

impl<T> DoubleEndedIterator for IterMut<'_, T> {
	fn next_back(&mut self) -> Option<Self::Item> {
		unsafe {
			self.inner.next_back().map(|ptr| ptr.as_mut().unwrap())
		}
	}
}

impl<T> ExactSizeIterator for IterMut<'_, T> {}
impl<T> FusedIterator for IterMut<'_, T> {}

pub struct IntoIter<T> {
	inner: RawIter<T>,
	_buffer: RawVec<T>
}

impl<T> Iterator for IntoIter<T> {
	type Item = T;

	fn next(&mut self) -> Option<Self::Item> {
		unsafe {
			self.inner.next().map(|ptr| ptr::read(ptr))
		}
	}

	fn size_hint(&self) -> (usize, Option<usize>) {
		self.inner.size_hint()
	}
}

impl<T> DoubleEndedIterator for IntoIter<T> {
	fn next_back(&mut self) -> Option<Self::Item> {
		unsafe {
			self.inner.next_back().map(|ptr| ptr::read(ptr))
		}
	}
}

impl<T> ExactSizeIterator for IntoIter<T> {}
impl<T> FusedIterator for IntoIter<T> {}
