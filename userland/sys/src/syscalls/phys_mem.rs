use bit_utils::Size;
use serde::{Serialize, Deserialize};

use crate::{
    CapId,
    CapType,
    CspaceTarget,
    KResult,
    syscall,
    sysret_1,
};
use crate::syscall_nums::*;
use super::{Capability, cap_destroy, WEAK_AUTO_DESTROY};

#[derive(Debug, Serialize, Deserialize)]
pub struct PhysMem {
    id: CapId,
    /// Size of memory, None if not known
    size: Option<Size>,
}

impl Capability for PhysMem {
    const TYPE: CapType = CapType::PhysMem;

    fn cloned_new_id(&self, cap_id: CapId) -> Option<Self> {
        Self::from_capid_size(cap_id, self.size)
    }

    fn cap_id(&self) -> CapId {
        self.id
    }
}

impl PhysMem {
    pub fn from_capid_size(cap_id: CapId, size: Option<Size>) -> Option<Self> {
        if cap_id.cap_type() == CapType::PhysMem {
            Some(PhysMem {
                id: cap_id,
                size,
            })
        } else {
            None
        }
    }

    pub fn refresh_size(&mut self) -> KResult<Size> {
        // panic safety: from_pages can panic, but syscall should not return invalid number of pages
        let size = unsafe {
            Size::from_pages(sysret_1!(syscall!(
                PHYS_MEM_GET_SIZE,
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
            None => self.refresh_size(),
        }
    }
}

impl Drop for PhysMem {
    fn drop(&mut self) {
        let _ = cap_destroy(CspaceTarget::Current, self.id);
    }
}