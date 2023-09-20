use serde::{Serialize, Deserialize};
use bit_utils::Size;

use crate::{
    CapId,
    CapType,
    CapFlags,
    KResult,
    CspaceTarget,
    syscall,
    sysret_1,
    sysret_2,
    EventPoolAwaitFlags,
};
use crate::syscall_nums::*;
use super::{Capability, Allocator, cap_destroy, WEAK_AUTO_DESTROY, INVALID_CAPID_MESSAGE};

#[derive(Debug, Serialize, Deserialize)]
pub struct EventPool(CapId);

impl Capability for EventPool {
    const TYPE: CapType = CapType::EventPool;

    fn from_cap_id(cap_id: CapId) -> Option<Self> {
        if cap_id.cap_type() == CapType::EventPool {
            Some(EventPool(cap_id))
        } else {
            None
        }
    }

    fn cap_id(&self) -> CapId {
        self.0
    }
}

/// Returned by [`await_event`], represents a range of event data that can be processed
#[derive(Debug, Clone, Copy)]
pub struct EventRange {
    pub data: *const u8,
    pub len: usize,
}

impl EventPool {
    pub fn new(allocator: &Allocator, max_size: Size) -> KResult<Self> {
        unsafe {
            sysret_1!(syscall!(
                EVENT_POOL_NEW,
                WEAK_AUTO_DESTROY,
                allocator.as_usize(),
                max_size.pages_rounded()
            )).map(|num| EventPool(CapId::try_from(num).expect(INVALID_CAPID_MESSAGE)))
        }
    }

    /// Waits for an event to occur, and returns a pointer to the event data slice
    pub fn await_event(&self, timeout: Option<u64>) -> KResult<EventRange> {
        let flags = match timeout {
            Some(_) => EventPoolAwaitFlags::TIMEOUT,
            _ => EventPoolAwaitFlags::empty(),
        };

        let (addr, size) = unsafe {
            sysret_2!(syscall!(
                EVENT_POOL_AWAIT,
                flags.bits() | WEAK_AUTO_DESTROY,
                self.as_usize(),
                timeout.unwrap_or_default(),
                0usize
            ))?
        };

        Ok(EventRange {
            data: addr as *const u8,
            len: size,
        })
    }
}

impl Drop for EventPool {
    fn drop(&mut self) {
        let _ = cap_destroy(CspaceTarget::Current, self.0);
    }
}