use sys::{CapId, EventId};
use bit_utils::Size;

use crate::prelude::*;
use crate::sched::ThreadRef;
use crate::container::Weak;
use crate::cap::memory::{Memory, MemoryCopySrc, MemoryWriter, MemoryWriteRegion};
use crate::cap::channel::{CapabilityWriter, CapabilityTransferInfo};

mod event_pool;
pub use event_pool::*;
mod message_capacity;
mod queue_event_emitter;

#[derive(Debug)]
pub struct UserspaceBuffer {
    /// The capability id the buffer was created from, stored here so send events can tell userspace correct id
    pub memory_id: CapId,
    pub memory: Weak<Memory>,
    pub offset: usize,
    pub buffer_size: usize,
}

impl UserspaceBuffer {
    pub fn new(memory_id: CapId, memory: Weak<Memory>, offset: usize, buffer_size: usize) -> Self {
        UserspaceBuffer {
            memory_id,
            memory,
            offset,
            buffer_size,
        }
    }

    /// Writes into the userspace buffer
    /// 
    /// # Returns
    /// 
    /// Number of bytes written
    pub fn copy_from<T: MemoryCopySrc>(&self, src: &T) -> Size {
        let Some(memory) = self.memory.upgrade() else {
            return Size::zero();
        };

        let memory_lock = memory.inner_read();

        memory_lock.copy_from(self.offset..(self.offset + self.buffer_size), src)
    }

    /// Like [`copy_from_buffer`], but also copies capabilties based on the data in the src buffer
    // FIXME: actually implement copying capabilities
    pub fn copy_channel_message_from_buffer<T: MemoryCopySrc>(
        &self,
        src_buffer: &T,
        cap_transfer_info: CapabilityTransferInfo,
    ) -> Size {
        let Some(memory) = self.memory.upgrade() else {
            return Size::zero();
        };

        let memory_lock = memory.inner_read();
        let Some(output_writer) = memory_lock.create_memory_writer(
            self.offset..(self.offset + self.buffer_size),
        ) else {
            return Size::zero();
        };

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

    fn copy_to(&self, writer: &mut impl MemoryWriter) -> Size {
        let Some(memory) = self.memory.upgrade() else {
            return Size::zero()
        };

        let memory_lock = memory.inner_read();

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

        write_size
    }
}

#[derive(Debug)]
pub struct ThreadListenerRef {
    pub thread: ThreadRef,
    pub event_buffer: UserspaceBuffer,
}

#[derive(Debug)]
pub struct EventPoolListenerRef {
    pub event_pool: Weak<EventPool>,
    pub event_id: EventId,
}

impl EventPoolListenerRef {
    pub fn write_event<T: MemoryCopySrc + ?Sized>(&self, src: &T) -> KResult<()> {
        let Some(event_pool) = self.event_pool.upgrade() else {
            return Err(SysErr::InvlWeak);
        };

        event_pool.write_event(self.event_id, src)
    }

    /// See [`EventListenerRef`] for details, this behaves exectly the same as an EventListenerRef with an event pool
    pub fn write_channel_message(&self, src: &UserspaceBuffer, cap_transfer_info: CapabilityTransferInfo) -> KResult<Option<Size>> {
        let Some(event_pool) = self.event_pool.upgrade() else {
            return Ok(None);
        };

        match event_pool.write_channel_event(self.event_id, src, cap_transfer_info) {
            // this error is treated as the sender is now invalid, move onto next one
            Err(SysErr::OutOfCapacity) => return Ok(None),
            Err(error) => return Err(error),
            _ => (),
        }

        // event pool write event always writes the whole event buffer, so return size of src
        Ok(Some(Size::from_bytes(src.buffer_size)))
    }
}

#[derive(Debug)]
pub enum EventListenerRef {
    Thread(ThreadListenerRef),
    EventPool {
        event_pool: EventPoolListenerRef,
        /// If this is true, the event pool should automatically be requed to listen to the event again
        auto_reque: bool,
    },
}

impl EventListenerRef {
    pub fn is_event_pool(&self) -> bool {
        matches!(self, EventListenerRef::EventPool { .. })
    }

    /// If the event listener should be requed after recieving an event
    pub fn is_auto_reque(&self) -> bool {
        match self {
            Self::Thread(_) => false,
            Self::EventPool { auto_reque, .. } => *auto_reque,
        }
    }
}