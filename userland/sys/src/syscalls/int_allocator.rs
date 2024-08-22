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
use super::{cap_destroy, Allocator, Capability, CapabilityIdListIterator, Interrupt, InterruptId, INVALID_CAPID_MESSAGE, WEAK_AUTO_DESTROY};

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

    /// Creates `count` interrupts with a given alignmant and returns an iterator over the created interrupts
    pub fn create_interrupts(&self, allocator: &Allocator, count: usize, align: usize) -> KResult<impl Iterator<Item = Interrupt>> {
        let (base_cap_id, _cpu_num, _interrupt_num) = unsafe {
            sysret_3!(syscall!(
                INTERRUPT_NEW,
                WEAK_AUTO_DESTROY,
                self.as_usize(),
                allocator.as_usize(),
                count,
                align
            ))?
        };

        let base_cap_id = CapId::try_from(base_cap_id).expect(INVALID_CAPID_MESSAGE);

        let iter = CapabilityIdListIterator::new(base_cap_id, count)
            .map(|interrupt_id| Interrupt::from_cap_id(interrupt_id).expect(INVALID_CAPID_MESSAGE));

        Ok(iter)
    }
}

impl Drop for IntAllocator {
    fn drop(&mut self) {
        let _ = cap_destroy(CspaceTarget::Current, self.0);
    }
}