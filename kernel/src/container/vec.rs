use core::ptr::NonNull;
use core::marker::PhantomData;
use core::alloc::Layout;

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
}
