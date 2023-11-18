use serde::{Serialize, Deserialize};
use bit_utils::Size;

use crate::{
    CapId,
    CapType,
    KResult,
    CspaceTarget,
    syscall,
    sysret_1, PhysMem,
};
use crate::syscall_nums::*;
use super::{Capability, Allocator, cap_destroy, WEAK_AUTO_DESTROY, INVALID_CAPID_MESSAGE};

#[derive(Debug, Serialize, Deserialize)]
pub struct MmioAllocator(CapId);

impl Capability for MmioAllocator {
    const TYPE: CapType = CapType::MmioAllocator;

    fn cloned_new_id(&self, cap_id: CapId) -> Option<Self> {
        Self::from_cap_id(cap_id)
    }

    fn cap_id(&self) -> CapId {
        self.0
    }
}

impl MmioAllocator {
    pub fn from_cap_id(cap_id: CapId) -> Option<Self> {
        if cap_id.cap_type() == CapType::MmioAllocator {
            Some(MmioAllocator(cap_id))
        } else {
            None
        }
    }

    pub fn alloc(&self, allocator: &Allocator, phys_addr: usize, size: Size) -> KResult<PhysMem> {
        let cap_id = unsafe {
            sysret_1!(syscall!(
                MMIO_ALLOCATOR_ALLOC,
                WEAK_AUTO_DESTROY,
                self.as_usize(),
                allocator.as_usize(),
                phys_addr,
                size.pages_rounded()
            ))?
        };

        let cap_id = CapId::try_from(cap_id).expect(INVALID_CAPID_MESSAGE);
        Ok(PhysMem::from_capid_size(cap_id, Some(size)).expect(INVALID_CAPID_MESSAGE))
    }
}

impl Drop for MmioAllocator {
    fn drop(&mut self) {
        let _ = cap_destroy(CspaceTarget::Current, self.0);
    }
}