use serde::{Serialize, Deserialize};
use bit_utils::Size;

use crate::{
    CapId,
    CapType,
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
pub struct EventPool {
    id: CapId,
    size: Size,
}

impl Capability for EventPool {
    const TYPE: CapType = CapType::EventPool;

    fn cloned_new_id(&self, cap_id: CapId) -> Option<Self> {
        if cap_id.cap_type() == CapType::EventPool {
            Some(EventPool {
                id: cap_id,
                size: self.size,
            })
        } else {
            None
        }
    }

    fn cap_id(&self) -> CapId {
        self.id
    }
}

/// Returned by [`await_event`], represents a range of event data that can be processed
#[derive(Debug, Clone, Copy)]
pub struct EventRange {
    pub data: *const u8,
    pub len: usize,
}

impl EventRange {
    /// Returns the slice of data this event range points to
    /// 
    /// # Safety
    /// 
    /// The returned slice is only valid as long as await_event is not called again on the event pool this came from
    /// Once it is called again, this returned slice is no longer valid
    pub unsafe fn as_slice(&self) -> &[u8] {
        unsafe {
            core::slice::from_raw_parts(self.data, self.len)
        }
    }
}

impl EventPool {
    pub fn new(allocator: &Allocator, max_size: Size) -> KResult<Self> {
        let cap_id = unsafe {
            sysret_1!(syscall!(
                EVENT_POOL_NEW,
                WEAK_AUTO_DESTROY,
                allocator.as_usize(),
                max_size.pages_rounded()
            ))?
        };

        Ok(EventPool {
            id: CapId::try_from(cap_id).expect(INVALID_CAPID_MESSAGE),
            size: max_size,
        })
    }

    pub fn size(&self) -> Size {
        self.size
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
        let _ = cap_destroy(CspaceTarget::Current, self.id);
    }
}