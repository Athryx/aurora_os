use core::ptr::NonNull;

use sys::MmioAllocator;
use bit_utils::{Size, align_up, align_down, PAGE_SIZE};
use aurora::prelude::*;
use aurora::{this_context, addr_space, allocator::addr_space::{MemoryMappingOptions, RegionPadding, MapPhysMemArgs}};
use volatile::VolatilePtr;

use crate::error::HwAccessError;

/// Provides methods for conveniant access to physical memory addressess
pub struct PmemAccess {
    pub allocator: MmioAllocator,
}

impl PmemAccess {
    /// This is only used in the acpi handler
    pub fn map_address_raw(&self, physical_address: usize, size: Size) -> Result<RawPmemData, HwAccessError> {
        let end_address = physical_address + size.bytes();

        let region_start_addr = align_down(physical_address, PAGE_SIZE);
        let region_end_addr = align_up(end_address, PAGE_SIZE);
        let region_size = Size::from_bytes(region_end_addr - region_start_addr);

        let phys_mem = self.allocator.alloc(&this_context().allocator, region_start_addr, region_size)?;

        let map_result = addr_space().map_phys_mem(MapPhysMemArgs {
            phys_mem,
            options: MemoryMappingOptions {
                read: true,
                write: true,
                ..Default::default()
            },
            address: None,
            padding: RegionPadding::default(),
        })?;

        // offset from start of physical region we mapped to the actual requested data
        let data_offset = physical_address - region_start_addr;

        Ok(RawPmemData {
            base_virt_address: map_result.address,
            data_offset,
            size: map_result.size,
        })
    }

    /// # Safety
    /// 
    /// Callers must ensure that physical address stores a valid type T
    pub unsafe fn map<T>(&self, physical_address: usize) -> Result<PmemData<T>, HwAccessError> {
        let raw_data = self.map_address_raw(physical_address, Size::from_bytes(core::mem::size_of::<T>()))?;
        let ptr = NonNull::new(
            (raw_data.base_virt_address + raw_data.data_offset) as *mut T,
        ).unwrap();

        Ok(PmemData {
            base_virt_address: raw_data.base_virt_address,
            ptr: VolatilePtr::new(ptr),
        })
    }
}

impl From<MmioAllocator> for PmemAccess {
    fn from(allocator: MmioAllocator) -> Self {
        PmemAccess {
            allocator,
        }
    }
}

/// A region of physical memory that has been mapped
pub struct RawPmemData {
    pub base_virt_address: usize,
    pub data_offset: usize,
    pub size: Size,
}

/// A pointer to a certain type that has been mapped in physical memory
pub struct PmemData<T: 'static> {
    base_virt_address: usize,
    ptr: VolatilePtr<'static, T>,
}

impl<T> PmemData<T> {
    pub fn ptr(&self) -> VolatilePtr<'static, T> {
        self.ptr
    }
}

impl<T> Drop for PmemData<T> {
    fn drop(&mut self) {
        unsafe {
            addr_space().unmap_memory(self.base_virt_address)
                .expect("could not unmap physical memoery");
        }
    }
}