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

    pub fn as_slice(&self) -> &[u8] {
        unsafe { core::slice::from_raw_parts(self.as_ptr(), self.size) }
    }

    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        unsafe { core::slice::from_raw_parts_mut(self.as_mut_ptr(), self.size) }
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

    pub fn copy_from_mem_offset(&mut self, other: &[u8], offset: usize) -> usize {
        if offset >= self.size() {
            return 0;
        }
    
        let size = min(self.size() - offset, other.len());
        unsafe {
            // safety: offset is checked to be less then size of this allocation
            let allocation_ptr = self.as_mut_ptr::<u8>().add(offset);

            let dst: &mut [u8] = core::slice::from_raw_parts_mut(allocation_ptr, size);
            let src: &[u8] = core::slice::from_raw_parts(other.as_ptr(), size);
            dst.copy_from_slice(src);
        }
        size
    }

    // returns number of bytes copied
    pub fn copy_from_mem(&mut self, other: &[u8]) -> usize {
        self.copy_from_mem_offset(other, 0)
    }
}