//! Event recivers used by channels only
//! 
//! All the types here usually wrap event listener ref with some extra data

use sys::{Event, MessageSent};

use crate::cap::capability_space::CapabilitySpace;
use crate::prelude::*;
use crate::container::{Arc, Weak};
use crate::event::{ThreadListenerRef, EventPoolListenerRef, UserspaceBuffer};
use crate::sched::WakeReason;
use super::CapabilityTransferInfo;


/// Similar to [`EventListenerRef`], but event pool variant also holds a buffer which says where the event should be sent from
/// 
/// Used for senders on channels
#[derive(Debug)]
pub enum ChannelSenderRef {
    Thread {
        sender: ThreadListenerRef,
        cspace: Weak<CapabilitySpace>,
    },
    EventPool {
        send_complete_event: EventPoolListenerRef,
        event_data: UserspaceBuffer,
        cspace: Weak<CapabilitySpace>,
    },
}

impl ChannelSenderRef {
    /// Notifies the sender that the channel message has been sent
    pub fn acknowledge_send(&self, write_size: Size) {
        match self {
            Self::Thread{ sender, .. } => {
                sender.thread.move_to_ready_list(
                    WakeReason::MsgSendRecv { msg_size: write_size }
                );
            },
            Self::EventPool { send_complete_event, event_data, .. } => {
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
            Self::Thread { sender, .. } => &sender.event_buffer,
            Self::EventPool { event_data, .. } => event_data,
        }
    }

    pub fn cspace(&self) -> Option<Arc<CapabilitySpace>> {
        let cspace = match self {
            Self::Thread { cspace, .. } => cspace,
            Self::EventPool { cspace, .. } => cspace,
        };

        cspace.upgrade()
    }
}

#[derive(Debug)]
pub enum ChannelRecieverRef {
    Thread {
        reciever: ThreadListenerRef,
        cspace: Weak<CapabilitySpace>,
    },
    EventPool {
        event_pool: EventPoolListenerRef,
        auto_reque: bool,
        cspace: Weak<CapabilitySpace>,
    }
}

impl ChannelRecieverRef {
    pub fn is_auto_reque(&self) -> bool {
        match self {
            Self::Thread { .. } => false,
            Self::EventPool { auto_reque, .. } => *auto_reque,
        }
    }

    /// Writes the data from the given buffer to the reciever
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
    pub fn write_channel_message(&self, src: &UserspaceBuffer, src_cspace: &CapabilitySpace) -> KResult<Option<Size>> {
        match self {
            Self::Thread {
                reciever,
                cspace,
            } => {
                let Some(cspace) = cspace.upgrade() else {
                    return Ok(None);
                };

                let write_size = reciever.event_buffer.copy_channel_message_from_buffer(src, CapabilityTransferInfo {
                    src_cspace,
                    dst_cspace: &cspace,
                });

                if !reciever.thread.move_to_ready_list(WakeReason::MsgSendRecv { msg_size: write_size }) {
                    // FIXME: drop capabilties that were copied over if the thread listener was invalid
                    Ok(None)
                } else {
                    Ok(Some(write_size))
                }
            },
            Self::EventPool {
                event_pool,
                cspace,
                ..
            } => {
                let Some(cspace) = cspace.upgrade() else {
                    return Ok(None);
                };

                event_pool.write_channel_message(src, CapabilityTransferInfo {
                    src_cspace,
                    dst_cspace: &cspace,
                })
            },
        }
    }
}