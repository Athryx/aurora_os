use core::sync::atomic::{AtomicPtr, Ordering};
use core::fmt::{self, Debug, Formatter};

use crate::prelude::*;
use crate::container::Vec;
use crate::alloc::{HeapAllocator, AllocRef};

// a non locking, synchronous vec
pub struct NLVec<T> {
	inner: AtomicPtr<Vec<*const T>>,
	allocer: AllocRef,
}

impl<T> NLVec<T>
{
	pub fn new(allocer: AllocRef) -> Self
	{
		let vec: Vec<*const T> = Vec::new();
		let ptr = to_heap(vec);
		NLVec(AtomicPtr::new(ptr))
	}

	pub fn len(&self) -> usize
	{
		self.read(|vec| vec.len())
	}

	pub fn is_empty(&self) -> bool
	{
		self.read(|vec| vec.is_empty())
	}

	pub fn get(&self, index: usize) -> Option<&T>
	{
		unsafe {
			self.read(|vec| vec.get(index).map(|ref_to_ptr| *ref_to_ptr))
				.map(|ptr| ptr.as_ref().unwrap())
		}
	}

	pub fn insert(&self, index: usize, element: T)
	{
		let ptr = to_heap(element);
		self.write(|vec| vec.insert(index, ptr));
	}

	pub fn push(&self, element: T)
	{
		let ptr = to_heap(element);
		self.write(|vec| vec.push(ptr));
	}

	pub fn remove(&self, index: usize) -> T
	{
		unsafe { from_heap(self.write(|vec| vec.remove(index))) }
	}

	pub fn pop(&self) -> Option<T>
	{
		unsafe { self.write(|vec| vec.pop()).map(|ptr| from_heap(ptr)) }
	}

	pub fn read<F, V>(&self, f: F) -> V
	where
		F: FnOnce(&Vec<*const T>) -> V,
	{
		unsafe { f(self.0.load(Ordering::Acquire).as_ref().unwrap()) }
	}

	pub fn write<F, V>(&self, mut f: F) -> V
	where
		F: FnMut(&mut Vec<*const T>) -> V,
	{
		loop {
			let ptr = self.0.load(Ordering::Acquire);
			let mut vec = unsafe { ptr.as_ref().unwrap().clone() };

			let out = f(&mut vec);

			let new_ptr = to_heap(vec);

			let result = self
				.0
				.compare_exchange(ptr, new_ptr, Ordering::AcqRel, Ordering::Acquire);
			match result {
				Ok(old_ptr) => {
					unsafe {
						// drop old value
						drop(Box::from_raw(old_ptr));
					}
					return out;
				},
				Err(_) => unsafe { drop(Box::from_raw(new_ptr)) },
			}
		}
	}
}

impl<T: Debug> Debug for NLVec<T>
{
	fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result
	{
		self.read(|vec| {
			write!(f, "[").unwrap();
			for (i, ptr) in vec.iter().enumerate() {
				let elem = unsafe { ptr.as_ref().unwrap() };
				write!(f, "{:?}", elem).unwrap();

				if i < vec.len() - 1 {
					write!(f, ", ").unwrap();
				}
			}
			write!(f, "]").unwrap();
		});
		Ok(())
	}
}
