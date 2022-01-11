// unless otherwise stated, all lens in this module are in bytes, not pages

pub mod addr;
pub mod range;
pub mod allocation;
pub mod page_layout;
pub mod mem_owner;

pub use addr::{PhysAddr, VirtAddr, phys_to_virt, virt_to_phys};
pub use range::*;
pub use allocation::{Allocation, HeapAllocation};
pub use page_layout::PageLayout;
pub use core::alloc::Layout;
pub use mem_owner::MemOwner;

pub unsafe fn init(mem_offset: usize) {
	unsafe {
		addr::set_mem_offset(mem_offset)
	}
}
