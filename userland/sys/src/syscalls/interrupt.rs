use serde::{Serialize, Deserialize};

use crate::{
    CapId,
    CapType,
    KResult,
    CspaceTarget,
    InterruptTrigger,
    syscall,
    sysret_2,
};
use crate::syscall_nums::*;
use super::{Capability, cap_destroy, WEAK_AUTO_DESTROY};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InterruptId {
    pub cpu_num: usize,
    pub interrupt_num: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Interrupt(CapId);

impl Capability for Interrupt {
    const TYPE: CapType = CapType::Interrupt;

    fn cloned_new_id(&self, cap_id: CapId) -> Option<Self> {
        Self::from_cap_id(cap_id)
    }

    fn cap_id(&self) -> CapId {
        self.0
    }
}

impl Interrupt {
    pub fn from_cap_id(cap_id: CapId) -> Option<Self> {
        if cap_id.cap_type() == CapType::Interrupt {
            Some(Interrupt(cap_id))
        } else {
            None
        }
    }

    // for now do not cache interrupt id, performance of retrieving interrupt id is not particularly important I imagine
    pub fn id(&self) -> KResult<InterruptId> {
        let (cpu_num, interrupt_num) = unsafe {
            sysret_2!(syscall!(
                INTERRUPT_ID,
                WEAK_AUTO_DESTROY,
                self.as_usize(),
                0usize,
                0usize
            ))?
        };

        Ok(InterruptId {
            cpu_num,
            interrupt_num,
        })
    }

    crate::generate_event_handlers!(
        InterruptTrigger,
        interrupt_trigger,
        INTERRUPT_HANDLE_INTERRUPT_TRIGGER_SYNC,
        INTERRUPT_HANDLE_INTERRUPT_TRIGGER_ASYNC,
        0
    );
}

impl Drop for Interrupt {
    fn drop(&mut self) {
        let _ = cap_destroy(CspaceTarget::Current, self.0);
    }
}