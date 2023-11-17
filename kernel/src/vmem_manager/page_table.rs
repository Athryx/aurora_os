//! Contains functions for creating and manipulating page tables
// FIXME: this module has some super unsafe code that should be fixed

use bitflags::bitflags;

use crate::prelude::*;
use crate::alloc::PaRef;
use crate::mem::{Allocation, PageLayout};
use super::PageMappingFlags;

/// Bitmask of page table entry address, all other bits are reserved or used for metadata bits
const PAGE_ADDR_BITMASK: usize = 0x000ffffffffff000;

pub const NUM_ENTRIES: usize = PAGE_SIZE / 8;

bitflags! {
    /// Bitmask of all the flags in a page table that the cpu uses
	pub struct PageTableFlags: usize {
		const PRESENT = 	1;
		const WRITABLE = 	1 << 1;
		const USER = 		1 << 2;
		const PWT = 		1 << 3;
		const PCD = 		1 << 4;
		const ACCESSED = 	1 << 5;
		const DIRTY = 		1 << 6;
		const HUGE = 		1 << 7;
		const GLOBAL = 		1 << 8;
		// this flag is an ignored bit, used by os to detemine if page table pointer
		// references a page table or if it maps a page directly
		const PAGE_TABLE =  1 << 9;
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
		let mut out = PageTableFlags::empty();
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

	pub fn is_page_table(&self) -> bool {
		self.flags().contains(PageTableFlags::PAGE_TABLE)
	}

	pub fn as_mut_ptr(&mut self) -> *mut PageTable {
		if self.0 & PageTableFlags::PRESENT.bits() == 0 {
			null_mut()
		} else {
			phys_to_virt(self.0 & PAGE_ADDR_BITMASK) as *mut PageTable
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
pub struct PageTable([PageTablePointer; NUM_ENTRIES]);

impl PageTable {
    /// Creates a new page table
    /// 
    /// Allocates the page table from `allocer`
    /// Returns a PageTablePointer with the provided `flags`
    /// Returns None if there is not enough memory
	pub fn new(
		allocer: &mut PaRef,
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
		let flags = flags | PageTableFlags::PRESENT | PageTableFlags::PAGE_TABLE;

		Some(PageTablePointer(addr | flags.bits()))
	}

    /// Returns the number of entries that are occupied in this page table
	pub fn entry_count(&self) -> usize {
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

	pub unsafe fn dealloc(&mut self, allocer: &mut PaRef) {
		let frame = Allocation::new(self.addr(), PAGE_SIZE);
		// TODO: maybe use regular dealloc and store the zindex in unused bits of page tabel entries
        unsafe { allocer.dealloc(frame); }
	}

	pub unsafe fn dealloc_all(&mut self, allocer: &mut PaRef) {
		unsafe { self.dealloc_recurse(allocer, 3); }
	}

    // FIXME: this is super unsafe
	unsafe fn dealloc_recurse(&mut self, allocer: &mut PaRef, level: usize) {
		let last_entry_index = self.0.len() - 1;

		if level > 0 {
			for (i, pointer) in self.0.iter_mut().enumerate() {
				// if level is 3 we are in the pml4 table so the higher half table pointer shouldn't be deallocated
				if pointer.flags().contains(PageTableFlags::HUGE) || (level == 3 && i == last_entry_index) {
					continue;
				}

                unsafe {
					if let Some(page_table) = pointer.as_mut_ptr().as_mut() {
						page_table.dealloc_recurse(allocer, level-1);
					}
                }
			}
		}

		unsafe { self.dealloc(allocer) }
	}

    /// Adds the page table entry at the given index
    /// 
    /// # Panics
    /// 
    /// panics if `index` is out of the page table bounds
	/// 
	/// # Safety
	/// 
	/// page tables cannot form a loop, it must be a tree
	/// (so a page table cannot have itself added as a child entry)
	pub unsafe fn add_entry(&mut self, index: usize, ptr: PageTablePointer) {
		let mut entry_count = self.entry_count();

		if !self.0[index].flags().present() {
			entry_count += 1;
		}

		self.0[index] = ptr;

		// do this after setting pointer, because setting pointer for index 0 could overwrite count
		self.set_entry_count(entry_count);
	}

	/// Returns a pointer to the page table at the given index, or null if it doesn't exist
	pub fn get(&mut self, index: usize) -> *mut PageTable {
		self.0[index].as_mut_ptr()
	}

	/// Returns a page table pointer to the table at the given index
	pub fn get_page_table_pointer(&self, index: usize) -> Option<PageTablePointer> {
		self.0.get(index).copied()
	}

	pub fn get_or_alloc<'a>(
		&'a mut self,
		index: usize,
		allocer: &mut PaRef,
		flags: PageTableFlags,
	) -> Option<&'a mut PageTable> {
		// safety: page tables form a tree (no recursive mapping)
		// so if we have mutable access to this table, it is not possible to get
		// mutable access to underlyinh pagetables any other way
		if self.present(index) {
			unsafe { self.0[index].as_mut_ptr().as_mut() }
		} else {
			let mut out = PageTable::new(allocer, flags)?;
			unsafe {
				self.add_entry(index, out);
				out.as_mut_ptr().as_mut()
			}
		}
	}

	/// Removes the page table entry at the `index` if it is present
    /// 
    /// # Panics
    /// 
    /// panics if `index` is out of the page table bounds
	pub fn remove(&mut self, index: usize) {
		if self.present(index) {
			self.0[index] = PageTablePointer(self.0[index].0 & !PageTableFlags::PRESENT.bits());
			self.inc_entry_count(-1);
		}
	}

    /// Returns the virtual address of this page table
	fn addr(&self) -> usize {
		self as *const _ as usize
	}
}
