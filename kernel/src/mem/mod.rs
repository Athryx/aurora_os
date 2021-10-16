// unless otherwise stated, all lens in this module are in bytes, not pages

pub mod addr;
pub mod range;
pub mod allocation;

pub use addr::{PhysAddr, VirtAddr, phys_to_virt, virt_to_phys};
pub use range::*;
pub use allocation::Allocation;

pub unsafe fn init(mem_offset: usize) {
	addr::set_mem_offset(mem_offset)
}
