//! Event recivers used by channels only
//! 
//! All the types here usually wrap event listener ref with some extra data

use sys::{EventData, MessageSent, EventId, Event};

use crate::cap::capability_space::CapabilitySpace;
use crate::prelude::*;
use crate::container::{Arc, Weak};
use crate::event::{EventPoolListenerRef, UserspaceBuffer, EventPool, WeakUserspaceBuffer};
use crate::sched::{WakeReason, ThreadRef};
use super::Reply;

#[derive(Debug)]
pub enum ChannelSenderInner {
    Thread {
        thread: Option<ThreadRef>,
    },
    EventPool {
        event_pool: Weak<EventPool>,
        event_id: EventId,
    },
    CallThread {
        thread: Option<ThreadRef>,
        recv_buffer: WeakUserspaceBuffer,
    },
    CallEventPool {
        event_pool: Weak<EventPool>,
        event_id: EventId,
    }
}

/// Reference to something which is sending oon a channel
#[derive(Debug)]
pub struct ChannelSenderRef {
    pub cspace: Weak<CapabilitySpace>,
    pub send_buffer: WeakUserspaceBuffer,
    pub inner: ChannelSenderInner,
}

impl ChannelSenderRef {
    pub fn current_thread(buffer: &UserspaceBuffer, cspace: &Arc<CapabilitySpace>) -> Self {
        ChannelSenderRef {
            cspace: Arc::downgrade(cspace),
            send_buffer: buffer.downgrade(),
            inner: ChannelSenderInner::Thread {
                thread: None,
            },
        }
    }

    pub fn event_pool(listener: EventPoolListenerRef, send_buffer: &UserspaceBuffer, cspace: &Arc<CapabilitySpace>) -> Self {
        let EventPoolListenerRef {
            event_pool,
            event_id,
        } = listener;

        ChannelSenderRef {
            cspace: Arc::downgrade(cspace),
            send_buffer: send_buffer.downgrade(),
            inner: ChannelSenderInner::EventPool {
                event_pool,
                event_id,
            },
        }
    }

    pub fn set_thread(&mut self, new_thread_ref: ThreadRef) {
        let thread = match &mut self.inner {
            ChannelSenderInner::Thread { thread: thread @ None } => thread,
            ChannelSenderInner::CallThread { thread: thread @ None, .. } => thread,
            _ => panic!("channel sender was not an empty thread"),
        };

        *thread = Some(new_thread_ref);
    }

    /// Notifies the sender that the channel message has been sent
    pub fn acknowledge_send(&self, write_size: Size) -> KResult<()> {
        match &self.inner {
            ChannelSenderInner::Thread{ thread: Some(sender_thread), .. } => {
                sender_thread.move_to_ready_list(
                    WakeReason::MsgSend { msg_size: write_size }
                );
            },
            ChannelSenderInner::EventPool { event_pool, event_id, .. } => {
                let event_pool = event_pool.upgrade().ok_or(SysErr::InvlWeak)?;

                let event_data = EventData::MessageSent(MessageSent {
                    recieved_size: write_size,
                });

                let event = Event {
                    event_data,
                    event_id: *event_id,
                }.as_raw();

                event_pool.write_event(event.as_bytes())?;
            },
            _ => (),
        }

        Ok(())
    }

    /// Gets the buffer that holds the data for the event to be sent, or None if the buffer has been dropped
    pub fn send_buffer(&self) -> Option<UserspaceBuffer> {
        self.send_buffer.upgrade()
    }

    pub fn cspace(&self) -> Option<Arc<CapabilitySpace>> {
        self.cspace.upgrade()
    }

    pub fn get_reply(&self, future_ref: Option<ThreadRef>) -> Option<Reply> {
        let reciever = match &self.inner {
            ChannelSenderInner::CallThread {
                thread,
                recv_buffer,
            } => {
                let thread = thread.clone().unwrap_or_else(|| {
                    future_ref.expect("future_ref cannot be None if sender thread is None")
                });

                ChannelRecieverRef::Thread {
                    thread: Some(thread),
                    message_buffer: recv_buffer.clone(),
                    cspace: self.cspace.clone(),
                }
            },
            ChannelSenderInner::CallEventPool {
                event_pool,
                event_id,
            } => ChannelRecieverRef::EventPool {
                event_pool: event_pool.clone(),
                event_id: *event_id,
                cspace: self.cspace.clone(),
                auto_reque: false,
            },
            _ => return None,
        };

        let reply = Reply::new(reciever);

        Some(reply)
    }
}

#[derive(Debug)]
pub enum ChannelRecieverRef {
    Thread {
        /// This is None if the recieving thread is the calling thread
        thread: Option<ThreadRef>,
        message_buffer: WeakUserspaceBuffer,
        cspace: Weak<CapabilitySpace>,
    },
    EventPool {
        event_pool: Weak<EventPool>,
        event_id: EventId,
        auto_reque: bool,
        cspace: Weak<CapabilitySpace>,
    }
}

impl ChannelRecieverRef {
    pub fn current_thread(buffer: &UserspaceBuffer, cspace: &Arc<CapabilitySpace>) -> Self {
        ChannelRecieverRef::Thread {
            thread: None,
            message_buffer: buffer.downgrade(),
            cspace: Arc::downgrade(cspace),
        }
    }

    pub fn event_pool(listener: EventPoolListenerRef, auto_reque: bool, cspace: &Arc<CapabilitySpace>) -> Self {
        let EventPoolListenerRef {
            event_pool,
            event_id,
        } = listener;

        ChannelRecieverRef::EventPool {
            event_pool,
            event_id,
            auto_reque,
            cspace: Arc::downgrade(cspace),
        }
    }

    pub fn set_thread(&mut self, new_thread_ref: ThreadRef) {
        let ChannelRecieverRef::Thread { thread: old_thread @ None, .. } = self else {
            panic!("channel reciever was not an empty thread");
        };

        *old_thread = Some(new_thread_ref);
    }

    pub fn is_auto_reque(&self) -> bool {
        match self {
            Self::Thread { .. } => false,
            Self::EventPool { auto_reque, .. } => *auto_reque,
        }
    }

    pub fn cspace(&self) -> Option<Arc<CapabilitySpace>> {
        let cspace = match self {
            ChannelRecieverRef::Thread { cspace, .. } => cspace,
            ChannelRecieverRef::EventPool { cspace, .. } => cspace,
        };

        cspace.upgrade()
    }
}