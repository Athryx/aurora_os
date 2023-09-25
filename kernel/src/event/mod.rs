use sys::{CapId, Event, EventId, MessageSent};
use bit_utils::Size;

use crate::prelude::*;
use crate::sched::{ThreadRef, WakeReason};
use crate::container::Weak;
use crate::cap::memory::{Memory, MemoryCopySrc, MemoryWriter};

mod broadcast_event_emitter;
mod event_pool;
pub use event_pool::*;
mod message_capacity;
mod queue_event_emitter;

#[derive(Debug)]
pub struct UserspaceBuffer {
    /// The capability id the buffer was created from, stored here so send events can tell userspace correct id
    memory_id: CapId,
    memory: Weak<Memory>,
    offset: usize,
    buffer_size: usize,
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
    /// 
    /// # Safety
    /// 
    /// Must not overwrite things that userspace is not expecting to be overwritten
    pub unsafe fn copy_from<T: MemoryCopySrc>(&self, src: &T) -> Size {
        let Some(memory) = self.memory.upgrade() else {
            return Size::zero();
        };

        let memory_lock = memory.inner_read();

        unsafe {
            memory_lock.copy_from(self.offset..(self.offset + self.buffer_size), src)
        }
    }

    /// Like [`copy_from_buffer`], but also copies capabilties based on the data in the src buffer
    // FIXME: actually implement copying capabilities
    pub unsafe fn copy_channel_message_from_buffer(&self, offset: usize, src: &UserspaceBuffer) -> Size {
        unsafe {
            self.copy_from(src)
        }
    }
}

impl MemoryCopySrc for UserspaceBuffer {
    fn size(&self) -> usize {
        self.buffer_size
    }

    unsafe fn copy_to(&self, writer: &mut MemoryWriter) -> Size {
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
            let write_result = unsafe {
                writer.write_region(vrange.as_unaligned())
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

    /// See [`EventListenerRef`] for details, this behaves exectly the same as and EventListenerRef with an event pool
    pub fn write_channel_message(&self, src: &UserspaceBuffer) -> KResult<Option<Size>> {
        let Some(event_pool) = self.event_pool.upgrade() else {
            return Ok(None);
        };

        match event_pool.write_event(self.event_id, src) {
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

    /// Writes the data from the given buffer to the event listener
    /// 
    /// This method also copies capabilities over
    /// 
    /// It will trigger the thread to wake up or the event pool to fire an event
    /// 
    /// # Returns
    /// 
    /// The number of bytes written, or Ok(None) if the listener was invalid
    /// 
    /// If any other error occured, Err is returned
    pub fn write_channel_message(&self, src: &UserspaceBuffer) -> KResult<Option<Size>> {
        match self {
            EventListenerRef::Thread(listener) => {
                let write_size = unsafe {
                    listener.event_buffer.copy_channel_message_from_buffer(0, src)
                };

                if !listener.thread.move_to_ready_list(WakeReason::MsgSendRecv { msg_size: write_size }) {
                    Ok(None)
                } else {
                    Ok(Some(write_size))
                }
            },
            EventListenerRef::EventPool { event_pool: event_pool_listener, .. } => {
                event_pool_listener.write_channel_message(src)
            },
        }
    }
}

/// Similar to [`EventListenerRef`], but event pool variant also holds a buffer which says where the event should be sent from
/// 
/// Used for senders on channels
#[derive(Debug)]
pub enum EventSenderRef {
    Thread(ThreadListenerRef),
    EventPool {
        send_complete_event: EventPoolListenerRef,
        event_data: UserspaceBuffer,
    },
}

impl EventSenderRef {
    /// Notifies the event sender that the event has been handled
    pub fn acknowledge_send(&self, write_size: Size) {
        match self {
            EventSenderRef::Thread(sender) => {
                sender.thread.move_to_ready_list(
                    WakeReason::MsgSendRecv { msg_size: write_size }
                );
            },
            EventSenderRef::EventPool { send_complete_event, event_data } => {
                let event = Event::MessageSent(MessageSent {
                    event_id: send_complete_event.event_id,
                    message_buffer_id: event_data.memory_id.into(),
                    message_buffer_offset: event_data.offset,
                    message_buffer_len: event_data.buffer_size,
                }).as_raw();

                // ignore errors, there is no where to report them to
                let _ = send_complete_event.write_event(event.as_bytes());
            },
        }
    }

    /// Gets the buffer that holds the data for the event to be sent
    pub fn event_buffer(&self) -> &UserspaceBuffer {
        match self {
            EventSenderRef::Thread(sender) => &sender.event_buffer,
            EventSenderRef::EventPool { event_data, .. } => event_data,
        }
    }
}