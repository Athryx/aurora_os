use core::cmp::min;

use crate::sched::ThreadRef;
use crate::container::Weak;
use crate::cap::memory::Memory;
use event_pool::BoundedEventPool;

mod broadcast_event_emitter;
mod event_pool;
mod message_capacity;
mod queue_event_emitter;

#[derive(Debug)]
pub struct UserspaceBuffer {
    memory: Weak<Memory>,
    offset: usize,
    buffer_size: usize,
}

impl UserspaceBuffer {
    /// Writes into the userspace buffer
    /// 
    /// # Returns
    /// 
    /// Number of bytes written, or none if the memory capability has been dropped
    /// 
    /// # Safety
    /// 
    /// Must not overwrite things that userspace is not expecting to be overwritten
    pub unsafe fn write(&self, data: &[u8], offset: usize) -> Option<usize> {
        let memory = self.memory.upgrade()?;

        if offset >= self.buffer_size {
            return Some(0);
        }

        let cap_offset = self.offset + offset;
        let write_size = min(data.len(), self.buffer_size - offset);

        let memory_lock = memory.inner_read();

        unsafe {
            Some(memory_lock.write(&data[..write_size], cap_offset))
        }
    }
}

#[derive(Debug)]
pub enum EventPoolListenerRef {
    BoundedEventPool(Weak<BoundedEventPool>),
}

#[derive(Debug)]
pub enum EventListenerRef {
    Thread {
        thread: ThreadRef,
        event_buffer: UserspaceBuffer,
    },
    EventPool(EventPoolListenerRef),
}

impl EventListenerRef {
    pub fn is_event_pool(&self) -> bool {
        matches!(self, EventListenerRef::EventPool(_))
    }
}