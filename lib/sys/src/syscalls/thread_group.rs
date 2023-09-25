use serde::{Serialize, Deserialize};

use crate::{
    CapId,
    CapType,
    KResult,
    CspaceTarget,
    syscall,
    sysret_0,
    sysret_1,
};
use crate::syscall_nums::*;
use super::{Capability, Allocator, cap_destroy, WEAK_AUTO_DESTROY, INVALID_CAPID_MESSAGE};

#[derive(Debug, Serialize, Deserialize)]
pub struct ThreadGroup(CapId);

impl Capability for ThreadGroup {
    const TYPE: CapType = CapType::ThreadGroup;

    fn cloned_new_id(&self, cap_id: CapId) -> Option<Self> {
        Self::from_cap_id(cap_id)
    }

    fn cap_id(&self) -> CapId {
        self.0
    }
}

impl ThreadGroup {
    pub fn from_cap_id(cap_id: CapId) -> Option<Self> {
        if cap_id.cap_type() == CapType::ThreadGroup {
            Some(ThreadGroup(cap_id))
        } else {
            None
        }
    }

    pub fn new_child_group(&self, allocator: &Allocator) -> KResult<Self> {
        let child_cap_id = unsafe {
            sysret_1!(syscall!(
                THREAD_GROUP_NEW,
                WEAK_AUTO_DESTROY,
                self.as_usize(),
                allocator.as_usize()
            ))?
        };

        Ok(ThreadGroup(CapId::try_from(child_cap_id).expect(INVALID_CAPID_MESSAGE)))
    }

    pub fn exit(&self) -> KResult<()> {
        unsafe {
            sysret_0!(syscall!(
                THREAD_GROUP_EXIT,
                WEAK_AUTO_DESTROY,
                self.as_usize()
            ))
        }
    }
}

impl Drop for ThreadGroup {
    fn drop(&mut self) {
        let _ = cap_destroy(CspaceTarget::Current, self.0);
    }
}