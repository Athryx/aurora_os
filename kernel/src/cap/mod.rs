use core::sync::atomic::{AtomicUsize, Ordering, fence};

use bitflags::bitflags;

use crate::prelude::*;
use crate::container::{Arc, Weak};
use crate::make_id_type_no_from;
use crate::alloc::OrigRef;

bitflags! {
	pub struct CapFlags: usize {
		const READ = 1;
		const PROD = 1 << 1;
		const WRITE = 1 << 2;
		const UPGRADE = 1 << 3;
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapType {
	Process,
	Memory,
	Event,
	Channel,
	Key,
	Interrupt,
	Port,
	Spawner,
	Allocator,
	RootOom,
	MmioAllocator,
	IntAllocator,
	PortAllocator,
}

impl CapType {
	pub fn from(n: usize) -> Option<Self> {
		Some(match n {
			0 => Self::Process,
			1 => Self::Memory,
			2 => Self::Event,
			3 => Self::Channel,
			4 => Self::Key,
			5 => Self::Interrupt,
			6 => Self::Port,
			7 => Self::Spawner,
			8 => Self::Allocator,
			9 => Self::RootOom,
			10 => Self::MmioAllocator,
			11 => Self::IntAllocator,
			12 => Self::PortAllocator,
			_ => return None,
		})
	}

	pub fn as_usize(&self) -> usize {
		*self as usize
	}
}

make_id_type_no_from!(CapId);

impl CapId {
	pub fn try_from(n: usize) -> Option<Self> {
		// fail if invalid type of cap object
		if get_bits(n, 5..9) > 12 {
			None
		} else {
			Some(CapId(n))
		}
	}

	pub fn flags(&self) -> CapFlags {
		CapFlags::from_bits_truncate(self.0)
	}

	pub fn is_weak(&self) -> bool {
		get_bits(self.0, 4..5) == 1
	}

	pub fn cap_type(&self) -> CapType {
		// panic safety: CapId will always have valid metadata, this is checked in the constructor
		CapType::from(get_bits(self.0, 5..9)).unwrap()
	}
}

impl Default for CapId {
	fn default() -> Self {
		// panic safety: 0 is a valid CapType
		Self::try_from(0).unwrap()
	}
}

pub trait CapObject {
	// called when the reference count on the CapObjectWrapper reaches 0
	fn cap_drop(&self);
}

// a wrapper around a cap object
// keeps track of the number of Capabilities referencing the cap object, and calls cap_drop when the refcount reaches 0
// this doesn't actually manage the objects memory, and it won't drop the underlying object when the refcount reaches 0, that is the job of the unerlying Arc
// it only keeps track of the refcount from the point of view of userspace
#[derive(Debug)]
struct CapObjectWrapper<T> {
	// we only need to keep track of strong count, since this doesn't manage the objects memory
	count: AtomicUsize,
	object: T,
}

impl<T: CapObject> CapObjectWrapper<T> {
	fn new(object: T) -> Self {
		CapObjectWrapper {
			count: AtomicUsize::new(1),
			object,
		}
	}
}

#[derive(Debug)]
pub struct Capability<T: CapObject> {
	object: Arc<CapObjectWrapper<T>>,
	flags: CapFlags,
	// if this is false, no refcounting will take place on the Capability object referenced by this Capability
	// refcounting for the memory will take place, but cap_drop will never be called
	// this will improve performance if refcounting is not needed
	do_refcount: bool,
}

impl<T: CapObject> Capability<T> {
	pub fn new(object: T, flags: CapFlags, allocer: OrigRef) -> KResult<Self> {
		let inner = Arc::new(CapObjectWrapper::new(object), allocer)?;
		Ok(Capability {
			object: inner,
			flags,
			do_refcount: true,
		})
	}

	pub fn new_no_refcount(object: T, flags: CapFlags, allocer: OrigRef) -> KResult<Self> {
		let inner = Arc::new(CapObjectWrapper::new(object), allocer)?;
		Ok(Capability {
			object: inner,
			flags,
			do_refcount: false,
		})
	}

	pub fn and_from_flags(cap: &Self, flags: CapFlags) -> Self {
		let mut out = cap.clone();
		out.flags &= flags;
		out
	}

	pub fn downgrade(&self) -> WeakCapability<T> {
		WeakCapability {
			object: Arc::downgrade(&self.object),
			flags: self.flags,
			do_refcount: self.do_refcount,
		}
	}

	pub fn object(&self) -> &T {
		&self.object.object
	}

	pub fn flags(&self) -> CapFlags {
		self.flags
	}

	pub fn is_refcounted(&self) -> bool {
		self.do_refcount
	}
}

impl<T: CapObject> Clone for Capability<T> {
	fn clone(&self) -> Self {
		if self.do_refcount {
			self.object.count.fetch_add(1, Ordering::Relaxed);
		}
		Capability {
			object: self.object.clone(),
			flags: self.flags,
			do_refcount: self.do_refcount,
		}
	}
}

impl<T: CapObject> Drop for Capability<T> {
	fn drop(&mut self) {
		if self.do_refcount {
			if self.object.count.fetch_sub(1, Ordering::Release) == 1 {
				fence(Ordering::Acquire);
				self.object.object.cap_drop();
			}
		}
	}
}

// default implementations of clone and drop are fine for this
#[derive(Debug)]
pub struct WeakCapability<T: CapObject> {
	object: Weak<CapObjectWrapper<T>>,
	flags: CapFlags,
	do_refcount: bool,
}

impl<T: CapObject> WeakCapability<T> {
	// fails if memory has been dropped or cap refcount is 0
	// NOTE: if do_refcount is false, this will succeeed if there is any arc pointing to the CapObject, even if there are no string capabilities
	pub fn upgrade(&self) -> Option<Capability<T>> {
		let arc = self.object.upgrade()?;
		if self.do_refcount {
			let mut count = arc.count.load(Ordering::Relaxed);

			loop {
				if count == 0 {
					return None;
				}

				match arc.count.compare_exchange_weak(count, count + 1, Ordering::Relaxed, Ordering::Relaxed) {
					Ok(_) => return Some(Capability {
						object: arc,
						flags: self.flags,
						do_refcount: self.do_refcount,
					}),
					Err(num) => count = num,
				}
			}
		} else {
			Some(Capability {
				object: arc,
				flags: self.flags,
				do_refcount: self.do_refcount,
			})
		}
	}

	pub fn and_from_flags(cap: &Self, flags: CapFlags) -> Self {
		let mut out: WeakCapability<T> = cap.clone();
		out.flags &= flags;
		out
	}

	pub fn flags(&self) -> CapFlags {
		self.flags
	}

	pub fn is_refcounted(&self) -> bool {
		self.do_refcount
	}
}

impl<T: CapObject> Clone for WeakCapability<T> {
	fn clone(&self) -> Self {
		WeakCapability {
			object: self.object.clone(),
			flags: self.flags,
			do_refcount: self.do_refcount,
		}
	}
}
