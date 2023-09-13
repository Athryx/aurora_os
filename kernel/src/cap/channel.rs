use bit_utils::MemOwner;
use bit_utils::container::{LinkedList, DefaultNode};
use sys::CapType;

use crate::alloc::HeapRef;
use crate::event::{EventListenerRef, UserspaceBuffer, EventSenderRef, ThreadListenerRef};
use crate::prelude::*;
use crate::mem::MemOwnerKernelExt;
use crate::sched::ThreadRef;
use crate::sync::{IMutex, IMutexGuard};
use super::CapObject;

#[derive(Debug, Default)]
struct ChannelInner {
    sender_queue: LinkedList<DefaultNode<EventSenderRef>>,
    reciever_queue: LinkedList<DefaultNode<EventListenerRef>>,
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
    pub fn try_send(&self, buffer: &UserspaceBuffer) -> Result<Size, SysErr> {
        let mut inner = self.inner();

        loop {
            let reciever = inner.reciever_queue.pop_front()
                .ok_or(SysErr::OkUnreach)?;

            let Some(write_size) = reciever.data.write_channel_message(buffer) else {
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
    pub fn try_recv(&self, buffer: &UserspaceBuffer) -> Result<Size, SysErr> {
        let mut inner = self.inner();

        loop {
            let sender = inner.sender_queue.pop_front()
                .ok_or(SysErr::OkUnreach)?;

            // TODO: detect if sender emmory capability is dropped
            let write_size = unsafe {
                buffer.copy_channel_message_from_buffer(0, sender.data.event_buffer())
            };

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
    pub fn sync_send(&self, buffer: UserspaceBuffer) -> SendRecvResult {
        let mut inner = self.inner();

        loop {
            let Some(reciever) = inner.reciever_queue.pop_front() else {
                // no recievers present, insert ourselves in the senders list
                let thread_ref = ThreadRef::future_ref(&cpu_local_data().current_thread());
                let sender = EventSenderRef::Thread(ThreadListenerRef {
                    thread: thread_ref,
                    event_buffer: buffer,
                });

                let sender = match MemOwner::new(sender.into(), &mut self.allocator.clone()) {
                    Ok(sender) => sender,
                    Err(error) => return SendRecvResult::Error(error),
                };

                inner.sender_queue.push(sender);

                return SendRecvResult::Block;
            };

            let Some(write_size) = reciever.data.write_channel_message(&buffer) else {
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
    pub fn sync_recv(&self, buffer: UserspaceBuffer) -> SendRecvResult {
        let mut inner = self.inner();

        loop {
            let Some(sender) = inner.sender_queue.pop_front() else {
                // no senders present, insert our selves in the recievers list
                let thread_ref = ThreadRef::future_ref(&cpu_local_data().current_thread());
                let reciever = EventListenerRef::Thread(ThreadListenerRef {
                    thread: thread_ref,
                    event_buffer: buffer,
                });

                let reciever = match MemOwner::new(reciever.into(), &mut self.allocator.clone()) {
                    Ok(reciever) => reciever,
                    Err(error) => return SendRecvResult::Error(error),
                };

                inner.reciever_queue.push(reciever);

                return SendRecvResult::Block;
            };

            // TODO: detect if sender memory capability is dropped
            let write_size = unsafe {
                buffer.copy_channel_message_from_buffer(0, sender.data.event_buffer())
            };

            sender.data.acknowledge_send(write_size);

            unsafe { sender.drop_in_place(&mut self.allocator.clone()); }

            return SendRecvResult::Success(write_size);
        }
    }
}

impl CapObject for Channel {
    const TYPE: CapType = CapType::Channel;
}