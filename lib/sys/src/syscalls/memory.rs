use serde::{Serialize, Deserialize};
use bit_utils::Size;

use crate::{
    CapId,
    CapType,
    CapFlags,
    KResult,
    CspaceTarget,
    syscall,
    sysret_1,
    sysret_2,
};
use crate::syscall_nums::*;
use super::{Capability, Allocator, cap_destroy, WEAK_AUTO_DESTROY, INVALID_CAPID_MESSAGE};

#[derive(Debug, Serialize, Deserialize)]
pub struct Memory {
    id: CapId,
    /// Size of memory, None if not known
    pub(super) size: Option<Size>,
}

impl Capability for Memory {
    const TYPE: CapType = CapType::Memory;

    fn from_cap_id(cap_id: CapId) -> Option<Self> {
        if cap_id.cap_type() == CapType::Memory {
            let out = Self {
                id: cap_id,
                size: None,
            };

            Some(out)
        } else {
            None
        }
    }

    fn cap_id(&self) -> CapId {
        self.id
    }
}

impl Memory {
    pub fn new(flags: CapFlags, allocator: &Allocator, size: Size) -> KResult<Self> {
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
}

impl Drop for Memory {
    fn drop(&mut self) {
        let _ = cap_destroy(CspaceTarget::Current, self.id);
    }
}