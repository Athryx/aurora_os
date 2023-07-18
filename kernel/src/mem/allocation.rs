use core::cmp::min;

use super::VirtAddr;
use crate::prelude::*;

#[derive(Debug, Clone, Copy)]
pub struct Allocation {
    ptr: VirtAddr,
    size: usize,
    /// specifies which PmemAllocator this allocation is from, or `None` if it is not known
    pub zindex: Option<usize>,
}

impl Allocation {
    // NOTE: panics if addr is not canonical
    pub fn new(addr: usize, size: usize) -> Self {
        Allocation {
            ptr: VirtAddr::new(addr),
            size,
            zindex: None,
        }
    }

    pub fn addr(&self) -> VirtAddr {
        self.ptr
    }

    pub fn as_ptr<T>(&self) -> *const T {
        self.ptr.as_ptr()
    }

    pub fn as_mut_ptr<T>(&mut self) -> *mut T {
        self.ptr.as_mut_ptr()
    }

    pub fn as_slice_ptr(&self) -> *const [u8] {
        core::ptr::from_raw_parts(self.as_ptr(), self.size)
    }

    pub fn as_mut_slice_ptr(&mut self) -> *mut [u8] {
        core::ptr::from_raw_parts_mut(self.as_mut_ptr(), self.size)
    }

    pub fn as_vrange(&self) -> UVirtRange {
        UVirtRange::new(self.ptr, self.size)
    }

    pub fn as_usize(&self) -> usize {
        self.ptr.as_usize()
    }

    pub fn size(&self) -> usize {
        self.size
    }

    /// # Returns
    /// 
    /// returns number of bytes copied
    /// 
    /// # Safety
    /// 
    /// `other` must not overlap with memory that is being written to
    pub unsafe fn copy_from_mem_offset(&mut self, offset: usize, data: *const [u8]) -> usize {
        if offset >= self.size() {
            return 0;
        }
    
        let size = min(self.size() - offset, data.len());
        unsafe {
            // safety: offset is checked to be less then size of this allocation
            let allocation_ptr = self.as_mut_ptr::<u8>().add(offset);

            // safety: caller must ensure that this allocation does not overlap with source array
            ptr::copy_nonoverlapping(data.as_ptr(), allocation_ptr, size);
        }
        size
    }

    /// # Returns
    /// 
    /// returns number of bytes copied
    /// 
    /// # Safety
    /// 
    /// `other` must not overlap with memory that is being written to
    pub unsafe fn copy_from_mem(&mut self, other: *const [u8]) -> usize {
        unsafe {
            self.copy_from_mem_offset(0, other)
        }
    }
}