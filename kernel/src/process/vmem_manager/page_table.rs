//! Contains functions for creating and manipulating page tables
// FIXME: this module has some super unsafe code that should be fixed

use bitflags::bitflags;

use crate::prelude::*;
use crate::alloc::PageAllocator;
use crate::mem::{Allocation, PageLayout};
use super::PageMappingFlags;

/// Bitmask of page table entry address, all other bits are reserved or used for metadata bits
const PAGE_ADDR_BITMASK: usize = 0x000ffffffffff000;

bitflags! {
    /// Bitmask of all the flags in a page table that the cpu uses
	pub struct PageTableFlags: usize {
		const NONE = 		0;
		const PRESENT = 	1;
		const WRITABLE = 	1 << 1;
		const USER = 		1 << 2;
		const PWT = 		1 << 3;
		const PCD = 		1 << 4;
		const ACCESSED = 	1 << 5;
		const DIRTY = 		1 << 6;
		const HUGE = 		1 << 7;
		const GLOBAL = 		1 << 8;
		const NO_EXEC =		1 << 63;
	}
}

impl PageTableFlags {
	fn present(&self) -> bool {
		self.contains(Self::PRESENT)
	}
}

impl From<PageMappingFlags> for PageTableFlags {
    fn from(flags: PageMappingFlags) -> Self {
		let mut out = PageTableFlags::NONE;
		if flags.contains(PageMappingFlags::WRITE) {
			out |= PageTableFlags::WRITABLE;
		}

		if !flags.contains(PageMappingFlags::EXEC) {
			out |= PageTableFlags::NO_EXEC;
		}

		if flags.exists() {
			out |= PageTableFlags::PRESENT;
		}

		if flags.contains(PageMappingFlags::USER) {
			out |= PageTableFlags::USER;
		}

		out
    }
}

#[repr(transparent)]
#[derive(Debug, Clone, Copy)]
pub struct PageTablePointer(usize);

impl PageTablePointer {
	pub fn new(addr: PhysAddr, flags: PageTableFlags) -> Self {
		let addr = addr.as_usize();
		PageTablePointer(addr | flags.bits())
	}

	unsafe fn as_ref<'a, 'b>(&'a self) -> Option<&'b PageTable> {
		if self.0 & PageTableFlags::PRESENT.bits() == 0 {
			None
		} else {
			let addr = phys_to_virt(self.0 & PAGE_ADDR_BITMASK);
            unsafe {
			    Some((addr as *const PageTable).as_ref().unwrap())
            }
		}
	}

	pub unsafe fn as_mut<'a, 'b>(&'a mut self) -> Option<&'b mut PageTable> {
		if self.0 & PageTableFlags::PRESENT.bits() == 0 {
			None
		} else {
			let addr = phys_to_virt(self.0 & PAGE_ADDR_BITMASK);
            unsafe {
			    Some((addr as *mut PageTable).as_mut().unwrap())
            }
		}
	}

	fn flags(&self) -> PageTableFlags {
		PageTableFlags::from_bits_truncate(self.0)
	}

	unsafe fn set_flags(&mut self, flags: PageTableFlags) {
		self.0 = (self.0 & PAGE_ADDR_BITMASK) | flags.bits();
	}

    /// Returns address of page table which this is pointing to
    pub fn address(&self) -> PhysAddr {
        PhysAddr::new(self.0 & PAGE_ADDR_BITMASK)
    }
}

#[repr(transparent)]
#[derive(Debug)]
pub struct PageTable([PageTablePointer; PAGE_SIZE / 8]);

impl PageTable {
    /// Creates a new page table
    /// 
    /// Allocates the page table from `allocer`
    /// Returns a PageTablePointer with the provided `flags`
    /// Returns None if there is not enough memory
	pub fn new(
		allocer: &dyn PageAllocator,
		flags: PageTableFlags,
	) -> Option<PageTablePointer> {
        // FIXME: handle case where allocator gives us back more than 1 frame
        // this is technically allowed to happen, but with current implementation it won't
		let frame = allocer.alloc(
            // This should never panic
            PageLayout::new_rounded(PAGE_SIZE, PAGE_SIZE).unwrap()
        )?.as_usize();

		unsafe {
			memset(frame as *mut u8, PAGE_SIZE, 0);
		}

		let addr = virt_to_phys(frame);
		let flags = flags | PageTableFlags::PRESENT;

		Some(PageTablePointer(addr | flags.bits()))
	}

    /// Returns the number of entries that are occupied in this page table
	fn entry_count(&self) -> usize {
        // the count is stored in the unused bits of the first entry
		get_bits(self.0[0].0, 52..63)
	}

	fn set_entry_count(&mut self, n: usize) {
		let n = get_bits(n, 0..11);
		let ptr_no_count = self.0[0].0 & 0x800fffffffffffff;
		self.0[0] = PageTablePointer(ptr_no_count | (n << 52));
	}

	fn inc_entry_count(&mut self, n: isize) {
		self.set_entry_count((self.entry_count() as isize + n) as _);
	}

    /// Returns true if a page table entry i present at the given index
    /// 
    /// # Panics
    /// 
    /// panics if `index` is out of the page table bounds
	fn present(&self, index: usize) -> bool {
		(self.0[index].0 & PageTableFlags::PRESENT.bits()) != 0
	}

	unsafe fn dealloc(&mut self, allocer: &dyn PageAllocator) {
		let frame = Allocation::new(self.addr(), PAGE_SIZE);
        unsafe { allocer.dealloc(frame); }
	}

	pub unsafe fn dealloc_all(&mut self, allocer: &dyn PageAllocator) {
		unsafe { self.dealloc_recurse(allocer, 3); }
	}

    // FIXME: this is super unsafe
	unsafe fn dealloc_recurse(&mut self, allocer: &dyn PageAllocator, level: usize) {
		if level > 0 {
			for pointer in self.0.iter_mut() {
				if pointer.flags().contains(PageTableFlags::HUGE) {
					continue;
				}

                unsafe {
                    pointer.as_mut()
                        .map(|page_table| page_table.dealloc_recurse(allocer, level-1));
                }
			}
		}

		unsafe { self.dealloc(allocer) }
	}

    /// Adds the page table entry at the given index
    /// 
    /// # Panics
    /// 
    /// panics if `index` is out of the page table bounds, or if the given index is already occupied
	pub fn add_entry(&mut self, index: usize, ptr: PageTablePointer) {
		assert!(!self.present(index));
		self.0[index] = ptr;
		self.inc_entry_count(1);
	}

	fn get<'a, 'b>(&'a mut self, index: usize) -> &'b mut PageTable {
		unsafe { self.0[index].as_mut().unwrap() }
	}

	fn get_or_alloc<'a>(
		&'a mut self,
		index: usize,
		allocer: &dyn PageAllocator,
		flags: PageTableFlags,
	) -> Option<&'a mut PageTable> {
		if self.present(index) {
			unsafe { self.0[index].as_mut() }
		} else {
			let mut out = PageTable::new(allocer, flags)?;
			self.add_entry(index, out);
			unsafe { out.as_mut() }
		}
	}

	/// Removes the page table entry at the `index` if it is present
    /// 
    /// # Panics
    /// 
    /// panics if `index` is out of the page table bounds
	unsafe fn remove<T: PageAllocator>(&mut self, index: usize) {
		if self.present(index) {
			self.0[index] = PageTablePointer(self.0[index].0 & !PageTableFlags::PRESENT.bits());
			self.inc_entry_count(-1);
		}
	}

    /// Returns the address of this page table
	fn addr(&self) -> usize {
		self as *const _ as usize
	}
}
