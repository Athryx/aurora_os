use serde::{Serialize, Deserialize};
use bit_utils::Size;

use crate::{
    CapId,
    CapType,
    CapFlags,
    KResult,
    ChannelSyncFlags,
    CspaceTarget,
    syscall,
    sysret_1,
};
use crate::syscall_nums::*;
use super::{Capability, Allocator, MessageBuffer, cap_destroy, WEAK_AUTO_DESTROY, INVALID_CAPID_MESSAGE};

#[derive(Debug, Serialize, Deserialize)]
pub struct Channel(CapId);

impl Capability for Channel {
    const TYPE: CapType = CapType::Channel;

    fn from_cap_id(cap_id: CapId) -> Option<Self> {
        if cap_id.cap_type() == CapType::Channel {
            Some(Channel(cap_id))
        } else {
            None
        }
    }

    fn cap_id(&self) -> CapId {
        self.0
    }
}

impl Channel {
    pub fn new(flags: CapFlags, allocator: &Allocator) -> KResult<Self> {
        unsafe {
            sysret_1!(syscall!(
                CHANNEL_NEW,
                flags.bits() as u32 | WEAK_AUTO_DESTROY,
                allocator.as_usize()
            )).map(|num| Channel(CapId::try_from(num).expect(INVALID_CAPID_MESSAGE)))
        }
    }

    pub fn try_send(&self, buffer: &MessageBuffer) -> KResult<Size> {
        assert!(buffer.is_readable());

        unsafe {
            sysret_1!(syscall!(
                CHANNEL_TRY_SEND,
                WEAK_AUTO_DESTROY,
                self.as_usize(),
                buffer.memory.as_usize(),
                buffer.offset.bytes(),
                buffer.size.bytes()
            )).map(Size::from_bytes)
        }
    }

    pub fn sync_send(&self, buffer: &MessageBuffer, timeout: Option<u64>) -> KResult<Size> {
        assert!(buffer.is_readable());

        let flags = match timeout {
            Some(_) => ChannelSyncFlags::TIMEOUT,
            None => ChannelSyncFlags::empty(),
        };

        unsafe {
            sysret_1!(syscall!(
                CHANNEL_SYNC_SEND,
                flags.bits() as u32 | WEAK_AUTO_DESTROY,
                self.as_usize(),
                buffer.memory.as_usize(),
                buffer.offset.bytes(),
                buffer.size.bytes(),
                timeout.unwrap_or_default()
            )).map(Size::from_bytes)
        }
    }

    pub fn try_recv(&self, buffer: &MessageBuffer) -> KResult<Size> {
        assert!(buffer.is_writable());

        unsafe {
            sysret_1!(syscall!(
                CHANNEL_TRY_RECV,
                WEAK_AUTO_DESTROY,
                self.as_usize(),
                buffer.memory.as_usize(),
                buffer.offset.bytes(),
                buffer.size.bytes()
            )).map(Size::from_bytes)
        }
    }

    pub fn sync_recv(&self, buffer: &MessageBuffer, timeout: Option<u64>) -> KResult<Size> {
        assert!(buffer.is_writable());

        let flags = match timeout {
            Some(_) => ChannelSyncFlags::TIMEOUT,
            None => ChannelSyncFlags::empty(),
        };

        unsafe {
            sysret_1!(syscall!(
                CHANNEL_SYNC_RECV,
                flags.bits() as u32 | WEAK_AUTO_DESTROY,
                self.as_usize(),
                buffer.memory.as_usize(),
                buffer.offset.bytes(),
                buffer.size.bytes(),
                timeout.unwrap_or_default()
            )).map(Size::from_bytes)
        }
    }
}

impl Drop for Channel {
    fn drop(&mut self) {
        let _ = cap_destroy(CspaceTarget::Current, self.0);
    }
}