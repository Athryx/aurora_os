use core::cmp::min;

use crate::container::Arc;
use crate::sched::{ThreadRef, thread_map, WakeReason};
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
    pub fn new(memory: Weak<Memory>, offset: usize, buffer_size: usize) -> Self {
        UserspaceBuffer {
            memory,
            offset,
            buffer_size,
        }
    }

    /// Writes into the userspace buffer
    /// 
    /// # Returns
    /// 
    /// Number of bytes written, or none if the memory capability has been dropped
    /// 
    /// # Safety
    /// 
    /// Must not overwrite things that userspace is not expecting to be overwritten
    pub unsafe fn write(&self, offset: usize, data: &[u8]) -> Option<usize> {
        let memory = self.memory.upgrade()?;

        if offset >= self.buffer_size {
            return Some(0);
        }

        let cap_offset = self.offset + offset;
        let write_size = min(data.len(), self.buffer_size - offset);

        let memory_lock = memory.inner_read();

        unsafe {
            Some(memory_lock.write(cap_offset,&data[..write_size]))
        }
    }

    /// Similar to [`write`], but gets the data to write from another userspace buffer instead of a slice
    pub unsafe fn copy_from_buffer(&self, offset: usize, src: &UserspaceBuffer) -> Option<usize> {
        let dst_memory = self.memory.upgrade()?;
        let src_memory = self.memory.upgrade()?;


        if offset >= self.buffer_size {
            return Some(0);
        }

        let dst_offset = self.offset + offset;
        let write_size = min(src.buffer_size, self.buffer_size - offset);

        let dst_lock = dst_memory.inner_read();
        let src_lock = src_memory.inner_read();

        unsafe {
            Some(dst_lock.copy_from_memory(
                dst_offset,
                &src_lock,
                src.offset..(src.offset + write_size)
            ))
        }
    }

    /// Like [`copy_from_buffer`], but also copies capabiltiesbased on the data in the src buffer
    // FIXME: actually implement copying capabilities
    pub unsafe fn copy_channel_message_from_buffer(&self, offset: usize, src: &UserspaceBuffer) -> Option<usize> {
        unsafe {
            self.copy_from_buffer(offset, src)
        }
    }
}

#[derive(Debug)]
pub struct ThreadListenerRef {
    pub thread: ThreadRef,
    pub event_buffer: UserspaceBuffer,
}

#[derive(Debug)]
pub enum EventPoolListenerRef {
    BoundedEventPool(Weak<BoundedEventPool>),
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
    /// The number of bytes written, or None if the write failed
    pub fn write_channel_message(&self, src: &UserspaceBuffer) -> Option<usize> {
        match self {
            EventListenerRef::Thread(listener) => {
                let write_size = unsafe {
                    listener.event_buffer.copy_channel_message_from_buffer(0, src)?
                };

                if !listener.thread.move_to_ready_list(WakeReason::MsgSendRecv { msg_size: write_size }) {
                    None
                } else {
                    Some(write_size)
                }
            },
            EventListenerRef::EventPool { .. } => todo!(),
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
    pub fn acknowledge_send(&self, write_size: usize) {
        match self {
            EventSenderRef::Thread(sender) => {
                sender.thread.move_to_ready_list(
                    WakeReason::MsgSendRecv { msg_size: write_size }
                );
            },
            EventSenderRef::EventPool { .. } => todo!(),
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