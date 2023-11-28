use serde::{Serialize, Deserialize};

use crate::{
    CapId,
    CapType,
    KResult,
    CspaceTarget,
    syscall,
    sysret_3,
};
use crate::syscall_nums::*;
use super::{Capability, Allocator, Interrupt, InterruptId, cap_destroy, WEAK_AUTO_DESTROY, INVALID_CAPID_MESSAGE};

#[derive(Debug, Serialize, Deserialize)]
pub struct IntAllocator(CapId);

impl Capability for IntAllocator {
    const TYPE: CapType = CapType::IntAllocator;

    fn cloned_new_id(&self, cap_id: CapId) -> Option<Self> {
        Self::from_cap_id(cap_id)
    }

    fn cap_id(&self) -> CapId {
        self.0
    }
}

impl IntAllocator {
    pub fn from_cap_id(cap_id: CapId) -> Option<Self> {
        if cap_id.cap_type() == CapType::IntAllocator {
            Some(IntAllocator(cap_id))
        } else {
            None
        }
    }

    pub fn create_interrupt(&self, allocator: &Allocator) -> KResult<(Interrupt, InterruptId)> {
        let (interrupt_cap_id, cpu_num, interrupt_num) = unsafe {
            sysret_3!(syscall!(
                INTERRUPT_NEW,
                WEAK_AUTO_DESTROY,
                self.as_usize(),
                allocator.as_usize(),
                0usize,
                0usize
            ))?
        };

        let interrupt_cap_id = CapId::try_from(interrupt_cap_id).expect(INVALID_CAPID_MESSAGE);
        let interrupt = Interrupt::from_cap_id(interrupt_cap_id).expect(INVALID_CAPID_MESSAGE);
        let interrupt_id = InterruptId {
            cpu_num,
            interrupt_num,
        };

        Ok((interrupt, interrupt_id))
    }
}

impl Drop for IntAllocator {
    fn drop(&mut self) {
        let _ = cap_destroy(CspaceTarget::Current, self.0);
    }
}