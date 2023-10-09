use sys::{CapId, Event, EventId, EventData};
use bit_utils::Size;

use crate::prelude::*;
use crate::container::Weak;
use crate::cap::memory::{Memory, MemoryCopySrc, MemoryWriter, MemoryWriteRegion};
use crate::cap::channel::{CapabilityWriter, CapabilityTransferInfo};
use crate::container::Arc;

mod broadcast_event_emitter;
pub use broadcast_event_emitter::*;
mod event_pool;
pub use event_pool::*;
mod message_capacity;
mod queue_event_emitter;

#[derive(Debug, Clone)]
pub struct WeakUserspaceBuffer {
    pub memory_id: CapId,
    pub memory: Weak<Memory>,
    pub offset: usize,
    pub buffer_size: usize,
}

impl WeakUserspaceBuffer {
    pub fn upgrade(&self) -> Option<UserspaceBuffer> {
        Some(UserspaceBuffer {
            memory_id: self.memory_id,
            memory: self.memory.upgrade()?,
            offset: self.offset,
            buffer_size: self.buffer_size,
        })
    }
}

#[derive(Debug, Clone)]
pub struct UserspaceBuffer {
    /// The capability id the buffer was created from, stored here so send events can tell userspace correct id
    pub memory_id: CapId,
    pub memory: Arc<Memory>,
    pub offset: usize,
    pub buffer_size: usize,
}

impl UserspaceBuffer {
    pub fn new(memory_id: CapId, memory: Arc<Memory>, offset: usize, buffer_size: usize) -> Self {
        UserspaceBuffer {
            memory_id,
            memory,
            offset,
            buffer_size,
        }
    }

    pub fn downgrade(&self) -> WeakUserspaceBuffer {
        WeakUserspaceBuffer {
            memory_id: self.memory_id,
            memory: Arc::downgrade(&self.memory),
            offset: self.offset,
            buffer_size: self.buffer_size,
        }
    }

    /// Writes into the userspace buffer
    /// 
    /// # Returns
    /// 
    /// Number of bytes written
    pub fn copy_from<T: MemoryCopySrc + ?Sized>(&self, src: &T) -> KResult<Size> {
        let memory_lock = self.memory.inner_read();

        memory_lock.copy_from(self.offset..(self.offset + self.buffer_size), src)
    }

    /// Like [`copy_from_buffer`], but also copies capabilties based on the data in the src buffer
    pub fn copy_channel_message_from_buffer<T: MemoryCopySrc>(
        &self,
        src_buffer: &T,
        cap_transfer_info: CapabilityTransferInfo,
    ) -> KResult<Size> {
        let memory_lock = self.memory.inner_read();
        let output_writer = memory_lock.create_memory_writer(
            self.offset..(self.offset + self.buffer_size),
        ).ok_or(SysErr::InvlMemZone)?;

        let mut capability_writer = CapabilityWriter::new(
            cap_transfer_info,
            output_writer,
        );

        src_buffer.copy_to(&mut capability_writer)
    }
}

impl MemoryCopySrc for UserspaceBuffer {
    fn size(&self) -> usize {
        self.buffer_size
    }

    fn copy_to(&self, writer: &mut impl MemoryWriter) -> KResult<Size> {
        let memory_lock = self.memory.inner_read();

        let region_iterator = memory_lock.iter_mapped_regions(
            VirtAddr::new(0),
            Size::from_bytes(self.offset),
            Size::from_bytes(self.buffer_size),
        );

        let mut write_size = Size::zero();
        for (vrange, _) in region_iterator {
            // safety: write region is created and used while we still have memory lock, it will remain valid
            let write_result = unsafe {
                writer.write_region(MemoryWriteRegion::from_vrange(vrange.as_unaligned()))
            };

            write_size += write_result.write_size;

            if write_result.end_reached {
                break;
            }
        }

        Ok(write_size)
    }
}

#[derive(Debug, Clone)]
pub struct EventPoolListenerRef {
    pub event_pool: Weak<EventPool>,
    pub event_id: EventId,
}

impl EventPoolListenerRef {
    pub fn write_event(&self, event_data: EventData) -> KResult<Size> {
        let event_pool = self.event_pool.upgrade().ok_or(SysErr::InvlWeak)?;

        let event = Event {
            event_data,
            event_id: self.event_id,
        }.as_raw();

        event_pool.write_event(event.as_bytes())
    }
}