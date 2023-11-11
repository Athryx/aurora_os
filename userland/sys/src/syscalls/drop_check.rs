use serde::{Serialize, Deserialize};

use crate::{
    CapId,
    CapType,
    CapDrop,
    KResult,
    CspaceTarget,
    syscall,
    sysret_2,
};
use crate::syscall_nums::*;
use super::{Capability, Allocator, cap_destroy, WEAK_AUTO_DESTROY, INVALID_CAPID_MESSAGE};

#[derive(Debug, Serialize, Deserialize)]
pub struct DropCheck(CapId);

impl Capability for DropCheck {
    const TYPE: CapType = CapType::DropCheck;

    fn cloned_new_id(&self, cap_id: CapId) -> Option<Self> {
        Self::from_cap_id(cap_id)
    }

    fn cap_id(&self) -> CapId {
        self.0
    }
}

impl DropCheck {
    fn from_cap_id(cap_id: CapId) -> Option<Self> {
        if cap_id.cap_type() == CapType::DropCheck {
            Some(DropCheck(cap_id))
        } else {
            None
        }
    }

    pub fn new(allocator: &Allocator, data: usize) -> KResult<(DropCheck, DropCheckReciever)> {
        let (drop_check_id, reciever_id) = unsafe {
            sysret_2!(syscall!(
                DROP_CHECK_NEW,
                WEAK_AUTO_DESTROY,
                allocator.as_usize(),
                data,
                0usize
            ))?
        };

        let drop_check = DropCheck(
            CapId::try_from(drop_check_id).expect(INVALID_CAPID_MESSAGE),
        );

        let reciever = DropCheckReciever(
            CapId::try_from(reciever_id).expect(INVALID_CAPID_MESSAGE),
        );

        Ok((drop_check, reciever))
    }
}

impl Drop for DropCheck {
    fn drop(&mut self) {
        let _ = cap_destroy(CspaceTarget::Current, self.0);
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DropCheckReciever(CapId);

impl Capability for DropCheckReciever {
    const TYPE: CapType = CapType::DropCheckReciever;

    fn cloned_new_id(&self, cap_id: CapId) -> Option<Self> {
        Self::from_cap_id(cap_id)
    }

    fn cap_id(&self) -> CapId {
        self.0
    }
}

impl DropCheckReciever {
    fn from_cap_id(cap_id: CapId) -> Option<Self> {
        if cap_id.cap_type() == CapType::DropCheckReciever {
            Some(DropCheckReciever(cap_id))
        } else {
            None
        }
    }

    crate::generate_event_handlers!(
        CapDrop,
        cap_drop,
        DROP_CHECK_RECIEVER_HANDLE_CAP_DROP_SYNC,
        DROP_CHECK_RECIEVER_HANDLE_CAP_DROP_ASYNC,
        1
    );
}

impl Drop for DropCheckReciever {
    fn drop(&mut self) {
        let _ = cap_destroy(CspaceTarget::Current, self.0);
    }
}