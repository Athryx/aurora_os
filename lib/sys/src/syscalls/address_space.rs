use serde::{Serialize, Deserialize};
use bit_utils::Size;

use crate::{
    CapId,
    CapType,
    KResult,
    MemoryMappingFlags,
    MemoryMapFlags,
    MemoryUpdateMappingFlags,
    MemoryResizeFlags,
    CspaceTarget,
    syscall,
    sysret_0,
    sysret_1,
};
use crate::syscall_nums::*;
use super::{Capability, Allocator, Memory, cap_destroy, WEAK_AUTO_DESTROY, INVALID_CAPID_MESSAGE};

#[derive(Debug, Serialize, Deserialize)]
pub struct AddressSpace(CapId);

impl Capability for AddressSpace {
    const TYPE: CapType = CapType::AddressSpace;

    fn from_cap_id(cap_id: CapId) -> Option<Self> {
        if cap_id.cap_type() == CapType::AddressSpace {
            Some(AddressSpace(cap_id))
        } else {
            None
        }
    }

    fn cap_id(&self) -> CapId {
        self.0
    }
}

impl Drop for AddressSpace {
    fn drop(&mut self) {
        let _ = cap_destroy(CspaceTarget::Current, self.0);
    }
}

impl AddressSpace {
    pub fn new(allocator: &Allocator) -> KResult<Self> {
        let addr_space_id = unsafe {
            sysret_1!(syscall!(
                ADDRESS_SPACE_NEW,
                WEAK_AUTO_DESTROY,
                allocator.as_usize()
            ))?
        };

        Ok(AddressSpace(
            CapId::try_from(addr_space_id)
                .expect(INVALID_CAPID_MESSAGE)
        ))
    }

    pub fn map_memory(&self, memory: &Memory, address: usize, max_size: Option<Size>, flags: MemoryMappingFlags) -> KResult<Size> {
        let mut flags = flags.bits() | WEAK_AUTO_DESTROY;
        if max_size.is_some() {
            flags |= MemoryMapFlags::MAX_SIZE.bits()
        }

        unsafe {
            sysret_1!(syscall!(
                MEMORY_MAP,
                flags,
                self.as_usize(),
                memory.as_usize(),
                address,
                max_size.unwrap_or_default().pages_rounded()
            )).map(Size::from_pages)
        }
    }

    pub fn unmap_memory(&self, address: usize) -> KResult<()> {
        unsafe {
            sysret_0!(syscall!(
                MEMORY_UNMAP,
                WEAK_AUTO_DESTROY,
                self.as_usize(),
                address
            ))
        }
    }

    pub fn update_memory_mapping(&self, address: usize, new_map_size: Option<Size>) -> KResult<Size> {
        let mut flags = MemoryUpdateMappingFlags::empty();
        if new_map_size.is_some() {
            flags |= MemoryUpdateMappingFlags::UPDATE_SIZE;
        }

        unsafe {
            sysret_1!(syscall!(
                MEMORY_UPDATE_MAPPING,
                flags.bits() | WEAK_AUTO_DESTROY,
                self.as_usize(),
                address,
                new_map_size.unwrap_or_default().pages_rounded()
            )).map(Size::from_pages)
        }
    }

    pub fn resize_memory(&self, memory: &mut Memory, new_size: Size, flags: MemoryResizeFlags) -> KResult<usize> {
        let new_size = unsafe {
            sysret_1!(syscall!(
                MEMORY_RESIZE,
                flags.bits(),
                self.as_usize(),
                memory.as_usize(),
                new_size.pages_rounded()
            ))
        }?;

        // panic safety: from_pages can panic, but syscall should not return invalid number of pages
        memory.size = Size::from_pages(new_size);

        Ok(new_size)
    }
}