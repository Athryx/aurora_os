use bit_utils::MemOwner;
use bit_utils::container::{LinkedList, DefaultNode};
use sys::CapType;

use crate::alloc::HeapRef;
use crate::event::{UserspaceBuffer, ThreadListenerRef, EventPoolListenerRef};
use crate::prelude::*;
use crate::mem::MemOwnerKernelExt;
use crate::sched::ThreadRef;
use crate::container::Arc;
use crate::sync::{IMutex, IMutexGuard};
use super::CapObject;
use super::capability_space::CapabilitySpace;

mod capability_writer;
pub use capability_writer::{CapabilityWriter, CapabilityTransferInfo};
mod event_listeners;
use event_listeners::{ChannelSenderRef, ChannelRecieverRef};

#[derive(Debug, Default)]
struct ChannelInner {
    sender_queue: LinkedList<DefaultNode<ChannelSenderRef>>,
    reciever_queue: LinkedList<DefaultNode<ChannelRecieverRef>>,
}

/// Returns result of channel functions to indicate to calling thread, success, failure or if it should block
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendRecvResult {
    /// A message of the given size in bytes was successfully sent or recieved without needing to block
    Success(Size),
    /// No message could be sent or recieved immediately, so the calling thread must block
    Block,
    /// An error occured
    Error(SysErr),
}

#[derive(Debug)]
pub struct Channel {
    inner: IMutex<ChannelInner>,
    allocator: HeapRef,
}

impl Channel {
    pub fn new(allocator: HeapRef) -> Self {
        Channel {
            inner: IMutex::default(),
            allocator,
        }
    }

    fn inner(&self) -> IMutexGuard<ChannelInner> {
        self.inner.lock()
    }

    // TODO: figure out when optimal time to lock channel is for all these methods
    // there could be more work done outside of the lock in some cases

    /// Trys to send a message if there is anything waiting to recieve the message
    /// 
    /// # Returns
    /// 
    /// Ok(number of bytes written) on success,
    /// Err if there was a nobody waiting to recieve the message
    pub fn try_send(&self, buffer: &UserspaceBuffer, src_cspace: &CapabilitySpace) -> KResult<Size> {
        let mut inner = self.inner();

        loop {
            let reciever = inner.reciever_queue.pop_front()
                .ok_or(SysErr::OkUnreach)?;

            let Some(write_size) = reciever.data.write_channel_message(buffer, src_cspace)? else {
                // this listener is no longer valid, retry on next listner
                unsafe { reciever.drop_in_place(&mut self.allocator.clone()); }
                continue;
            };

            if reciever.data.is_auto_reque() {
                inner.reciever_queue.push(reciever);
            } else {
                unsafe { reciever.drop_in_place(&mut self.allocator.clone()); }
            }

            return Ok(write_size);
        }
    }

    /// Trys to recieve a message if there is anything waiting to send the message
    /// 
    /// # Returns
    /// 
    /// Ok(number of bytes recieved) on success,
    /// Err if there was a nobody waiting to send the message
    pub fn try_recv(&self, buffer: &UserspaceBuffer, dst_cspace: &CapabilitySpace) -> Result<Size, SysErr> {
        let mut inner = self.inner();

        loop {
            let sender = inner.sender_queue.pop_front()
                .ok_or(SysErr::OkUnreach)?;

            let Some(src_cspace) = sender.data.cspace() else {
                // if csapce is not valid, try on next listener
                unsafe { sender.drop_in_place(&mut self.allocator.clone()); }
                continue;
            };

            // TODO: detect if sender emmory capability is dropped
            let write_size = buffer
                .copy_channel_message_from_buffer(sender.data.event_buffer(), CapabilityTransferInfo {
                    src_cspace: &src_cspace,
                    dst_cspace,
                });

            sender.data.acknowledge_send(write_size);

            unsafe { sender.drop_in_place(&mut self.allocator.clone()); }

            return Ok(write_size);
        }
    }

    /// Sends a message synchrounously
    /// 
    /// The calling thread may need to block to send the message
    /// 
    /// # Returns
    /// 
    /// See [`SendRecvResult`]
    pub fn sync_send(&self, buffer: UserspaceBuffer, src_cspace: Arc<CapabilitySpace>) -> SendRecvResult {
        let mut inner = self.inner();

        loop {
            let Some(reciever) = inner.reciever_queue.pop_front() else {
                // no recievers present, insert ourselves in the senders list
                let thread_ref = ThreadRef::future_ref(&cpu_local_data().current_thread());
                let thread_listener_ref = ThreadListenerRef {
                    thread: thread_ref,
                    event_buffer: buffer,
                };

                let sender = ChannelSenderRef::Thread {
                    sender: thread_listener_ref,
                    cspace: Arc::downgrade(&src_cspace),
                };

                let sender = match MemOwner::new(sender.into(), &mut self.allocator.clone()) {
                    Ok(sender) => sender,
                    Err(error) => return SendRecvResult::Error(error),
                };

                inner.sender_queue.push(sender);

                return SendRecvResult::Block;
            };

            let recieve_result = match reciever.data.write_channel_message(&buffer, &src_cspace) {
                Ok(recieve_result) => recieve_result,
                Err(error) => return SendRecvResult::Error(error),
            };

            let Some(write_size) = recieve_result else {
                // this listener is no longer valid, retry on next listner
                unsafe { reciever.drop_in_place(&mut self.allocator.clone()); }
                continue;
            };

            if reciever.data.is_auto_reque() {
                inner.reciever_queue.push(reciever);
            } else {
                unsafe { reciever.drop_in_place(&mut self.allocator.clone()); }
            }

            return SendRecvResult::Success(write_size);
        }
    }

    /// Recieves a message synchrounously
    /// 
    /// The calling thread may need to block to recieve the message
    /// 
    /// # Returns
    /// 
    /// See [`SendRecvResult`]
    pub fn sync_recv(&self, buffer: UserspaceBuffer, dst_cspace: Arc<CapabilitySpace>) -> SendRecvResult {
        let mut inner = self.inner();

        loop {
            let Some(sender) = inner.sender_queue.pop_front() else {
                // no senders present, insert our selves in the recievers list
                let thread_ref = ThreadRef::future_ref(&cpu_local_data().current_thread());
                let thread_listener_ref = ThreadListenerRef {
                    thread: thread_ref,
                    event_buffer: buffer,
                };

                let reciever = ChannelRecieverRef::Thread {
                    reciever: thread_listener_ref,
                    cspace: Arc::downgrade(&dst_cspace),
                };

                let reciever = match MemOwner::new(reciever.into(), &mut self.allocator.clone()) {
                    Ok(reciever) => reciever,
                    Err(error) => return SendRecvResult::Error(error),
                };

                inner.reciever_queue.push(reciever);

                return SendRecvResult::Block;
            };

            let Some(src_cspace) = sender.data.cspace() else {
                // move on to next listener if cspace is invalid
                unsafe { sender.drop_in_place(&mut self.allocator.clone()); }
                continue;
            };

            // TODO: detect if sender memory capability is dropped
            let write_size = buffer
                .copy_channel_message_from_buffer(sender.data.event_buffer(), CapabilityTransferInfo {
                    src_cspace: &src_cspace,
                    dst_cspace: &dst_cspace,
                });

            sender.data.acknowledge_send(write_size);

            unsafe { sender.drop_in_place(&mut self.allocator.clone()); }

            return SendRecvResult::Success(write_size);
        }
    }

    pub fn async_send(&self, event_pool: EventPoolListenerRef, message_buffer: UserspaceBuffer, src_cspace: Arc<CapabilitySpace>) -> KResult<()> {
        let mut inner = self.inner();

        loop {
            let Some(reciever) = inner.reciever_queue.pop_front() else {
                // no recievers present, insert ourselves in recievers queue
                let sender = ChannelSenderRef::EventPool {
                    send_complete_event: event_pool,
                    event_data: message_buffer,
                    cspace: Arc::downgrade(&src_cspace),
                };

                let sender = MemOwner::new(sender.into(), &mut self.allocator.clone())?;

                inner.sender_queue.push(sender);

                return Ok(());
            };

            let recieve_result = reciever.data.write_channel_message(&message_buffer, &src_cspace)?;

            let Some(write_size) = recieve_result else {
                // this listener is no longer valid, retry on next listner
                unsafe { reciever.drop_in_place(&mut self.allocator.clone()); }
                continue;
            };

            if reciever.data.is_auto_reque() {
                inner.reciever_queue.push(reciever);
            } else {
                unsafe { reciever.drop_in_place(&mut self.allocator.clone()); }
            }

            ChannelSenderRef::EventPool {
                send_complete_event: event_pool,
                event_data: message_buffer,
                // TODO: don't require cloning cspace just to send acknowledge event to event pool
                cspace: Arc::downgrade(&src_cspace),
            }.acknowledge_send(write_size);

            return Ok(());
        }
    }

    pub fn async_recv(&self, event_pool: EventPoolListenerRef, auto_reque: bool, dst_cspace: Arc<CapabilitySpace>) -> KResult<()> {
        let mut inner = self.inner();

        loop {
            let Some(sender) = inner.sender_queue.pop_front() else {
                // no senders present, insert ourselves in reciever queue
                let reciever = ChannelRecieverRef::EventPool {
                    event_pool,
                    auto_reque,
                    cspace: Arc::downgrade(&dst_cspace),
                };

                let reciever = MemOwner::new(reciever.into(), &mut self.allocator.clone())?;

                inner.reciever_queue.push(reciever);

                return Ok(());
            };

            let Some(src_cspace) = sender.data.cspace() else {
                // cspace is invalid, move onto next sender
                unsafe { sender.drop_in_place(&mut self.allocator.clone()); }
                continue;
            };

            let write_size = event_pool.write_channel_message(sender.data.event_buffer(), CapabilityTransferInfo {
                src_cspace: &src_cspace,
                dst_cspace: &dst_cspace,
            })?;

            match write_size {
                Some(write_size) => {
                    sender.data.acknowledge_send(write_size);
                    unsafe { sender.drop_in_place(&mut self.allocator.clone()); }
                    return Ok(());
                },
                None => {
                    // recv failed, try on recv on next sender
                    unsafe { sender.drop_in_place(&mut self.allocator.clone()); }
                    continue;
                }
            }
        }
    }
}

impl CapObject for Channel {
    const TYPE: CapType = CapType::Channel;
}