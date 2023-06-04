use core::{ptr::NonNull, ptr, ops::Deref};

use thiserror_no_std::Error;

use sys::Memory;

#[derive(Debug, Error)]
pub enum AddrSpaceError {
    #[error("Failed to update memory region list: out of memory")]
    RegionListOom,
}

struct MappedRegion {
    memory_cap: Memory,
    addr: usize,
    size: usize,
}

/// Behaves similar to a Vec<MappedRegion>, except instead of using the allocator, it uses memory mapping syscalls
struct RegionList {
    memory_cap: Memory,
    data: NonNull<MappedRegion>,
    len: usize,
    cap: usize,
}

impl RegionList {
    /// Doubles the size of the region list to allow space for more entries
    fn try_grow(&mut self) -> Result<(), AddrSpaceError> {
        todo!()
    }

    /// Ensures the region list has space for 1 more element
    fn ensure_capacity(&mut self) -> Result<(), AddrSpaceError> {
        if self.len == self.cap {
            self.try_grow()
        } else {
            Ok(())
        }
    }

    // returns a mutable pointer to the object at the specified index
    unsafe fn off(&mut self, index: usize) -> *mut MappedRegion {
        unsafe { self.data.as_ptr().add(index) }
    }

    fn insert(&mut self, index: usize, region: MappedRegion) -> Result<(), AddrSpaceError> {
        assert!(index <= self.len);

        self.ensure_capacity()?;

        let ncpy = self.len - index;

        unsafe {
            ptr::copy(self.off(index), self.off(index + 1), ncpy);
            ptr::write(self.off(index), region);
        }

        self.len += 1;

        Ok(())
    }

    pub fn remove(&mut self, index: usize) -> MappedRegion {
        assert!(index < self.len, "index out of bounds");

        let out = unsafe { ptr::read(self.off(index)) };

        self.len -= 1;
        let ncpy = self.len - index;

        unsafe {
            ptr::copy(self.off(index + 1), self.off(index), ncpy);
        }

        out
    }
}

impl Deref for RegionList {
    type Target = [MappedRegion];

    fn deref(&self) -> &Self::Target {
        unsafe {
            core::slice::from_raw_parts(self.data.as_ptr(), self.len)
        }
    }
}

pub struct AddrSpaceManager {
    region_list: RegionList,
}