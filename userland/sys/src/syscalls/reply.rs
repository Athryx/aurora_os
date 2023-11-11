use serde::{Serialize, Deserialize};
use bit_utils::Size;

use crate::{
    CapId,
    CapType,
    CspaceTarget,
    MessageBuffer,
    KResult,
    sysret_1,
    syscall,
};
use crate::syscall_nums::*;

use super::{Capability, cap_destroy, WEAK_AUTO_DESTROY};

#[derive(Debug, Serialize, Deserialize)]
pub struct Reply(CapId);

impl Capability for Reply {
    const TYPE: CapType = CapType::Reply;

    fn cloned_new_id(&self, cap_id: CapId) -> Option<Self> {
        Self::from_cap_id(cap_id)
    }

    fn cap_id(&self) -> CapId {
        self.0
    }
}

impl Reply {
    pub fn from_cap_id(cap_id: CapId) -> Option<Self> {
        if cap_id.cap_type() == CapType::Reply {
            Some(Reply(cap_id))
        } else {
            None
        }
    }

    pub fn from_usize(id: usize) -> Option<Self> {
        let cap_id = CapId::try_from(id)?;

        Self::from_cap_id(cap_id)
    }

    pub fn reply(self, send_buffer: &MessageBuffer) -> KResult<Size> {
        assert!(send_buffer.is_readable());

        let reply_size = unsafe {
            sysret_1!(syscall!(
                REPLY_REPLY,
                WEAK_AUTO_DESTROY,
                self.as_usize(),
                usize::from(send_buffer.memory_id),
                send_buffer.offset.bytes(),
                send_buffer.size.bytes()
            ))?
        };

        // kernel drops reply object when REPLY_REPLY is called
        core::mem::forget(self);

        Ok(Size::from_bytes(reply_size))
    }
}

impl Drop for Reply {
    fn drop(&mut self) {
        let _ = cap_destroy(CspaceTarget::Current, self.0);
    }
}