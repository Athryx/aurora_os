use serde::{Serialize, Deserialize};
use bit_utils::Size;

use crate::{
    CapId,
    CapType,
    CapFlags,
    KResult,
    syscall,
    sysret_1,
    sysret_2,
};
use crate::syscall_nums::*;
use super::{Capability, Allocator, WEAK_AUTO_DESTROY, INVALID_CAPID_MESSAGE};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Memory {
    id: CapId,
    /// Size of memory
    pub(super) size: Size,
}

impl Capability for Memory {
    const TYPE: CapType = CapType::Memory;

    fn from_cap_id(cap_id: CapId) -> Option<Self> {
        if cap_id.cap_type() == CapType::Memory {
            let mut out = Self {
                id: cap_id,
                size: Size::default(),
            };

            out.refresh_size().ok()?;

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
    pub fn new(flags: CapFlags, allocator: Allocator, size: Size) -> KResult<Self> {
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
                size: Size::from_pages(size),
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
        self.size = unsafe {
            Size::from_pages(sysret_1!(syscall!(
                MEMORY_GET_SIZE,
                WEAK_AUTO_DESTROY,
                self.as_usize()
            ))?)
        };

        Ok(self.size)
    }

    pub fn size(&self) -> Size {
        self.size
    }
}