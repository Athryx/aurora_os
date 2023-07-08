use core::cmp::min;

use crate::sched::ThreadRef;
use crate::container::Weak;
use crate::cap::memory::Memory;
use event_pool::BoundedEventPool;

mod broadcast_event_emitter;
mod event_pool;
mod queue_event_emitter;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WriteResult {
    FullWrite,
    PartialWrite,
    WriteError,
}

#[derive(Debug)]
struct UserspaceBuffer {
    memory: Weak<Memory>,
    offset: usize,
    buffer_size: usize,
}

impl UserspaceBuffer {
    /// 
    fn write(&self, data: &[u8], offset: usize) -> Option<usize> {
        let memory = self.memory.upgrade()?;

        if offset >= self.buffer_size {
            return Some(0);
        }

        let cap_offset = self.offset + offset;
        let write_size = min(data.len(), self.buffer_size - offset);

        let mut memory_lock = memory.inner();

        unsafe {
            Some(memory_lock.write(&data[..write_size], cap_offset))
        }
    }
}

#[derive(Debug)]
pub enum EventListenerRef {
    Thread(ThreadRef),
    BoundedEventPool(Weak<BoundedEventPool>),
}