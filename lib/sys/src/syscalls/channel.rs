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
    sysret_0,
    sysret_1, ChannelAsyncRecvFlags,
};
use crate::syscall_nums::*;
use super::{Capability, Allocator, MessageBuffer, EventPool, cap_destroy, WEAK_AUTO_DESTROY, INVALID_CAPID_MESSAGE};

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
                flags.bits() | WEAK_AUTO_DESTROY,
                self.as_usize(),
                buffer.memory.as_usize(),
                buffer.offset.bytes(),
                buffer.size.bytes(),
                timeout.unwrap_or_default()
            )).map(Size::from_bytes)
        }
    }

    pub fn async_send(&self, buffer: &MessageBuffer, event_pool: &EventPool) -> KResult<()> {
        assert!(buffer.is_readable());

        unsafe {
            sysret_0!(syscall!(
                CHANNEL_ASYNC_SEND,
                WEAK_AUTO_DESTROY,
                self.as_usize(),
                buffer.memory.as_usize(),
                buffer.offset.bytes(),
                buffer.size.bytes(),
                event_pool.as_usize()
            ))
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
                flags.bits() | WEAK_AUTO_DESTROY,
                self.as_usize(),
                buffer.memory.as_usize(),
                buffer.offset.bytes(),
                buffer.size.bytes(),
                timeout.unwrap_or_default()
            )).map(Size::from_bytes)
        }
    }

    pub fn async_recv(&self, event_pool: &EventPool, auto_reque: bool) -> KResult<()> {
        let flags = if auto_reque {
            ChannelAsyncRecvFlags::AUTO_REQUE
        } else {
            ChannelAsyncRecvFlags::empty()
        };

        unsafe {
            sysret_0!(syscall!(
                CHANNEL_ASYNC_RECV,
                flags.bits() | WEAK_AUTO_DESTROY,
                self.as_usize(),
                event_pool.as_usize()
            ))
        }
    }
}

impl Drop for Channel {
    fn drop(&mut self) {
        let _ = cap_destroy(CspaceTarget::Current, self.0);
    }
}