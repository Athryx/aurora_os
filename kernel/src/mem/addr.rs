use core::ops::{Add, AddAssign, Sub, SubAssign};

use crate::prelude::*;

// mutex is too slow
// offset of kernel virtual memory mapping of physical memory
static mut MEM_OFFSET: usize = 0;

pub fn phys_to_virt(addr: usize) -> usize {
	unsafe {
		// safety: to_virt will check safety of virtual address
		PhysAddr::new_unchecked(addr).to_virt().as_usize()
	}
}

pub fn virt_to_phys(addr: usize) -> usize {
	unsafe {
		// safety: to_phys will check safety of physical address
		VirtAddr::new_unchecked(addr).to_phys().as_usize()
	}
}

pub unsafe fn set_mem_offset(offset: usize) {
	MEM_OFFSET = offset;
}

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct PhysAddr(usize);

impl PhysAddr {
	const MASK: usize = 0x003fffffffffffff;

	pub fn new(addr: usize) -> Self {
		Self::try_new(addr).expect("invalid physical addres")
	}

	pub fn try_new(addr: usize) -> Option<Self> {
		match get_bits(addr, 48..64) {
			0 => Some(PhysAddr(addr)),
			_ => None,
		}
	}

	pub fn new_truncate(addr: usize) -> Self {
		PhysAddr(addr & Self::MASK)
	}

	pub unsafe fn new_unchecked(addr: usize) -> Self {
		PhysAddr(addr)
	}

	pub fn to_virt(self) -> VirtAddr {
		match VirtAddr::try_new(self.0 + unsafe { MEM_OFFSET }) {
			Some(addr) => addr,
			None => panic!("could not convert physical address {:x} to virtual address", self.0),
		}
	}
}

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct VirtAddr(usize);

impl VirtAddr {
	const MASK: usize = 0x0003ffffffffffff;

	pub fn new(addr: usize) -> Self {
		Self::try_new(addr).expect("invalid virtual addres")
	}

	pub fn try_new(addr: usize) -> Option<Self> {
		match get_bits(addr, 47..64) {
			0 => Some(VirtAddr(addr)),
			1 => Some(Self::new_truncate(addr)),
			_ => None,
		}
	}

	pub fn new_truncate(addr: usize) -> Self {
		match get_bits(addr, 47..48) {
			0 => VirtAddr(addr & Self::MASK),
			1 => VirtAddr(addr | !Self::MASK),
			_ => unreachable!(),
		}
	}

	pub unsafe fn new_unchecked(addr: usize) -> Self {
		VirtAddr(addr)
	}

	pub fn to_phys(self) -> PhysAddr {
		let addr = self.0;
		if addr < unsafe { MEM_OFFSET } {
			panic!("could not convert virtual address {:x} to physical address", addr);
		}
		PhysAddr::new(addr - unsafe { MEM_OFFSET })
	}
}

macro_rules! addr_methods {
	($addr:ident) => {
		impl $addr {
			pub fn as_usize(&self) -> usize {
				self.0
			}

			pub fn align_down(self, align: usize) -> Self {
				unsafe {
					Self::new_unchecked(align_down(self.0, align))
				}
			}

			// panics if align up alignes out of bounds
			pub fn align_up(self, align: usize) -> Self {
				Self::new(align_up(self.0, align))
			}

			pub fn as_ptr<T>(self) -> *const T {
				self.0 as *const T
			}

			pub fn as_mut_ptr<T>(self) -> *mut T {
				self.0 as *mut T
			}
		}

		impl Add<usize> for $addr {
			type Output = Self;

			fn add(self, rhs: usize) -> Self {
				Self::new(self.0 + rhs)
			}
		}

		impl AddAssign<usize> for $addr {
			fn add_assign(&mut self, rhs: usize) {
				*self = Self::new(self.0 + rhs)
			}
		}

		impl Sub<usize> for $addr {
			type Output = Self;

			fn sub(self, rhs: usize) -> Self {
				Self::new(self.0 - rhs)
			}
		}

		impl Sub<$addr> for $addr {
			type Output = usize;

			fn sub(self, rhs: Self) -> usize {
				self.0 - rhs.0
			}
		}

		impl SubAssign<usize> for $addr {
			fn sub_assign(&mut self, rhs: usize) {
				*self = Self::new(self.0 - rhs)
			}
		}
	};
}

addr_methods!(PhysAddr);
addr_methods!(VirtAddr);
