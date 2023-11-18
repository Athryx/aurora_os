use serde::{Serialize, Deserialize};
use bit_utils::Size;

use crate::{
    CapId,
    CapType,
    KResult,
    CspaceTarget,
    syscall,
    sysret_1,
    sysret_2,
    MemoryNewFlags,
    MemoryResizeFlags,
};
use crate::syscall_nums::*;
use super::{Capability, Allocator, cap_destroy, WEAK_AUTO_DESTROY, INVALID_CAPID_MESSAGE};

#[derive(Debug, Serialize, Deserialize)]
pub struct Memory {
    id: CapId,
    /// Size of memory, None if not known
    size: Option<Size>,
}

impl Capability for Memory {
    const TYPE: CapType = CapType::Memory;

    fn cloned_new_id(&self, cap_id: CapId) -> Option<Self> {
        Self::from_capid_size(cap_id, self.size)
    }

    fn cap_id(&self) -> CapId {
        self.id
    }
}

impl Memory {
    pub fn from_capid_size(cap_id: CapId, size: Option<Size>) -> Option<Self> {
        if cap_id.cap_type() == CapType::Memory {
            Some(Self {
                id: cap_id,
                size,
            })
        } else {
            None
        }
    }

    pub fn new(allocator: &Allocator, size: Size, flags: MemoryNewFlags) -> KResult<Self> {
        unsafe {
            sysret_2!(syscall!(
                MEMORY_NEW,
                flags.bits() as u32 | WEAK_AUTO_DESTROY,
                allocator.as_usize(),
                size.pages_rounded(),
                // FIXME: hack to make syscall macro return right amount of values
                0 as usize
            )).map(|(cap_id, size)| Memory {
                id: CapId::try_from(cap_id).expect(INVALID_CAPID_MESSAGE),
                size: Some(Size::from_pages(size)),
            })
        }
    }

    /// Updates the size field using `memory_get_size` syscall
    /// 
    /// # Returns
    /// 
    /// The new size of the memory in pages
    pub fn refresh_size(&mut self) -> KResult<Size> {
        // panic safety: from_pages can panic, but syscall should not return invalid number of pages
        let size = unsafe {
            Size::from_pages(sysret_1!(syscall!(
                MEMORY_GET_SIZE,
                WEAK_AUTO_DESTROY,
                self.as_usize()
            ))?)
        };

        self.size = Some(size);

        Ok(size)
    }

    pub fn size(&mut self) -> KResult<Size> {
        match self.size {
            Some(size) => Ok(size),
            None => self.refresh_size()
        }
    }

    pub fn resize(&mut self, new_size: Size, flags: MemoryResizeFlags) -> KResult<usize> {
        let new_size = unsafe {
            sysret_1!(syscall!(
                MEMORY_RESIZE,
                flags.bits(),
                self.as_usize(),
                new_size.pages_rounded()
            ))
        }?;

        // panic safety: from_pages can panic, but syscall should not return invalid number of pages
        self.size = Some(Size::from_pages(new_size));

        Ok(new_size)
    }
}

impl Drop for Memory {
    fn drop(&mut self) {
        let _ = cap_destroy(CspaceTarget::Current, self.id);
    }
}