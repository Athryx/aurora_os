use core::cmp::max;

use crate::prelude::*;

/// Like layout, but for pages
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageLayout {
	size: usize,
	align: usize,
}

impl PageLayout {
	/// Rounds up size to the nearest page size
	/// Sets align to page size if it is less than page size
	/// Returns None if the rounded size overflows when aligning up
	pub fn new_rounded(size: usize, align: usize) -> Option<Self> {
		let size = align_up(size, PAGE_SIZE);
		let align = max(align, PAGE_SIZE);

		if size > usize::MAX - (align - 1) {
			None
		} else {
			unsafe {
				Some(Self::from_size_align_unchecked(size, align))
			}
		}
	}

	/// Returns None if:
	/// size is not page aligned
	/// align is not a power of 2 or alignmant specified is less than page alignmant
	/// rounding up align overflows
	pub fn from_size_align(size: usize, align: usize) -> Option<Self> {
		if !align.is_power_of_two() ||
			size > usize::MAX - (align - 1) ||
			align_of(size) < PAGE_SIZE ||
			align_of(align) < PAGE_SIZE {
			None
		} else  {
			unsafe {
				Some(Self::from_size_align_unchecked(size, align))
			}
		}
	}

	pub unsafe fn from_size_align_unchecked(size: usize, align: usize) -> Self {
		PageLayout {
			size,
			align,
		}
	}

	pub fn size(&self) -> usize {
		self.size
	}

	pub fn align(&self) -> usize {
		self.align
	}
}
