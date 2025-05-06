use core::convert::Infallible;
use core::ops::FromResidual;

use bit_utils::MemOwner;
use bit_utils::container::{LinkedList, DefaultNode};
use sys::{CapType, CapId, CapFlags};

use crate::alloc::HeapRef;
use crate::event::{UserspaceBuffer, EventPoolListenerRef};
use crate::prelude::*;
use crate::mem::MemOwnerKernelExt;
use crate::sched::{ThreadRef, WakeReason, thread_map};
use crate::container::Arc;
use crate::sync::{IMutex, IMutexGuard};

use super::{CapObject, StrongCapability, Capability};
use super::capability_space::CapabilitySpace;

mod capability_writer;
pub use capability_writer::{CapabilityWriter, CapabilityTransferInfo};
mod event_listeners;
use event_listeners::{ChannelSenderRef, ChannelSenderInner, ChannelRecieverRef};
mod reply;
pub use reply::Reply;

/// Data from a recieve operation
#[derive(Debug, Clone, Copy)]
pub struct RecieveResult {
    pub recieve_size: Size,
    pub reply_cap_id: Option<CapId>,
}

/// Returns result of synchronous channel functions to indicate to calling thread, success, failure or if it should block
pub enum ChannelSyncResult<T> {
    /// A message was succesfully sent or recieved without needing to block
    Success(T),
    /// No message could be sent or recieved immediately, so the calling thread must block
    Block,
    /// An error occured
    Error(SysErr),
}

impl<T> FromResidual<KResult<Infallible>> for ChannelSyncResult<T> {
    fn from_residual(residual: KResult<Infallible>) -> Self {
        match residual {
            Ok(a) => match a {},
            Err(error) => ChannelSyncResult::Error(error),
        }
    }
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

    fn insert_reply_to_cspace(&self, reply: Reply, cspace: &CapabilitySpace) -> KResult<CapId> {
        let reply_capability = StrongCapability::new_flags(
            Arc::new(
                reply,
                self.allocator.clone(),
            )?,
            CapFlags::WRITE,
        );

        Ok(cspace.insert_reply(Capability::Strong(reply_capability))?)
    }

    // TODO: figure out when optimal time to lock channel is for all these methods
    // there could be more work done outside of the lock in some cases

    /// Trys to send a message if there is anything waiting to recieve the message
    /// 
    /// # Returns
    /// 
    /// Ok(number of bytes written) on success,
    /// Err if there was a nobody waiting to recieve the message
    pub fn try_send(&self, buffer: &UserspaceBuffer, src_cspace: &Arc<CapabilitySpace>) -> KResult<Size> {
        let sender = ChannelSenderRef::current_thread(buffer, src_cspace);

        let mut inner = self.inner();

        loop {
            let reciever = inner.reciever_queue.pop_front()
                .ok_or(SysErr::OkUnreach)?;
            let reciever = unsafe { reciever.as_box(self.allocator.clone()) };

            let Ok(recieve_result) = self.do_send(&sender, &reciever.data, None) else {
                // this listener is no longer valid, retry on next listner
                continue;
            };

            if reciever.data.is_auto_reque() {
                inner.reciever_queue.push(Box::into_mem_owner(reciever));
            }

            return Ok(recieve_result.recieve_size);
        }
    }

    /// Trys to recieve a message if there is anything waiting to send the message
    /// 
    /// # Returns
    /// 
    /// Ok(number of bytes recieved) on success,
    /// Err if there was a nobody waiting to send the message
    pub fn try_recv(&self, buffer: &UserspaceBuffer, dst_cspace: &Arc<CapabilitySpace>) -> KResult<RecieveResult> {
        let reciever = ChannelRecieverRef::current_thread(buffer, dst_cspace);

        let mut inner = self.inner();

        loop {
            let sender = inner.sender_queue.pop_front()
                .ok_or(SysErr::OkUnreach)?;
            let sender = unsafe { sender.as_box(self.allocator.clone()) };

            let Ok(recieve_result) = self.do_send(&sender.data, &reciever, None) else {
                continue;
            };

            return Ok(recieve_result);
        }
    }

    /// Sends a message synchrounously
    /// 
    /// The calling thread may need to block to send the message
    /// 
    /// # Returns
    /// 
    /// See [`ChannelSyncResult`]
    pub fn sync_send(&self, buffer: &UserspaceBuffer, src_cspace: &Arc<CapabilitySpace>) -> ChannelSyncResult<Size> {
        let mut sender = ChannelSenderRef::current_thread(buffer, src_cspace);
        let current_thread = ThreadRef::future_ref(&cpu_local_data().current_thread());

        let mut inner = self.inner();

        loop {
            let Some(reciever) = inner.reciever_queue.pop_front() else {
                // no recievers present, insert ourselves in the senders list
                sender.set_thread(current_thread);

                let sender = MemOwner::new(sender.into(), &mut self.allocator.clone())?;
                inner.sender_queue.push(sender);

                return ChannelSyncResult::Block;
            };
            let reciever = unsafe { reciever.as_box(self.allocator.clone()) };

            let Ok(recieve_result) = self.do_send(&sender, &reciever.data, None) else {
                continue;
            };

            if reciever.data.is_auto_reque() {
                inner.reciever_queue.push(Box::into_mem_owner(reciever));
            }

            return ChannelSyncResult::Success(recieve_result.recieve_size);
        }
    }

    /// Recieves a message synchrounously
    /// 
    /// The calling thread may need to block to recieve the message
    /// 
    /// # Returns
    /// 
    /// See [`ChannelSyncResult`]
    pub fn sync_recv(&self, buffer: &UserspaceBuffer, dst_cspace: &Arc<CapabilitySpace>) -> ChannelSyncResult<RecieveResult> {
        let mut reciever = ChannelRecieverRef::current_thread(buffer, dst_cspace);
        let current_thread = ThreadRef::future_ref(&cpu_local_data().current_thread());

        let mut inner = self.inner();

        loop {
            let Some(sender) = inner.sender_queue.pop_front() else {
                // no senders present, insert our selves in the recievers list
                reciever.set_thread(current_thread);

                let reciever = MemOwner::new(reciever.into(), &mut self.allocator.clone())?;
                inner.reciever_queue.push(reciever);

                return ChannelSyncResult::Block;
            };
            let sender = unsafe { sender.as_box(self.allocator.clone()) };

            let Ok(recieve_result) = self.do_send(&sender.data, &reciever, None) else {
                continue;
            };

            return ChannelSyncResult::Success(recieve_result);
        }
    }

    pub fn async_send(&self, listener: EventPoolListenerRef, send_buffer: &UserspaceBuffer, src_cspace: &Arc<CapabilitySpace>) -> KResult<()> {
        let sender = ChannelSenderRef::event_pool(listener, send_buffer, src_cspace);

        let mut inner = self.inner();

        loop {
            let Some(reciever) = inner.reciever_queue.pop_front() else {
                let sender = MemOwner::new(sender.into(), &mut self.allocator.clone())?;
                inner.sender_queue.push(sender);

                return Ok(());
            };
            let reciever = unsafe { reciever.as_box(self.allocator.clone()) };

            let Ok(_) = self.do_send(&sender, &reciever.data, None) else {
                continue;
            };

            if reciever.data.is_auto_reque() {
                inner.reciever_queue.push(Box::into_mem_owner(reciever));
            }

            return Ok(());
        }
    }

    pub fn async_recv(&self, listener: EventPoolListenerRef, auto_reque: bool, dst_cspace: &Arc<CapabilitySpace>) -> KResult<()> {
        let reciever = ChannelRecieverRef::event_pool(listener, auto_reque, dst_cspace);

        let mut inner = self.inner();

        loop {
            let Some(sender) = inner.sender_queue.pop_front() else {
                // no senders present, insert ourselves in reciever queue
                let reciever = MemOwner::new(reciever.into(), &mut self.allocator.clone())?;
                inner.reciever_queue.push(reciever);

                return Ok(());
            };
            let sender = unsafe { sender.as_box(self.allocator.clone()) };

            let Ok(_) = self.do_send(&sender.data, &reciever, None) else {
                continue;
            };

            // NOTE: this could report failure when trying to listen for a message,
            // but the message may still have been successfully sent
            if reciever.is_auto_reque() {
                let reciever = MemOwner::new(reciever.into(), &mut self.allocator.clone())?;
                inner.reciever_queue.push(reciever);
            }

            return Ok(());
        }
    }

    /// It is always required to block after calling this
    pub fn sync_call(&self, send_buffer: &UserspaceBuffer, recv_buffer: &UserspaceBuffer, cspace: &Arc<CapabilitySpace>) -> KResult<()> {
        let mut sender = ChannelSenderRef {
            cspace: Arc::downgrade(cspace),
            send_buffer: send_buffer.downgrade(),
            inner: ChannelSenderInner::CallThread {
                thread: None,
                recv_buffer: recv_buffer.downgrade(),
            },
        };
        let current_thread = ThreadRef::future_ref(&cpu_local_data().current_thread());

        let mut inner = self.inner();

        loop {
            let Some(reciever) = inner.reciever_queue.pop_front() else {
                sender.set_thread(current_thread);

                let sender = MemOwner::new(sender.into(), &mut self.allocator.clone())?;
                inner.sender_queue.push(sender);

                return Ok(());
            };
            let reciever = unsafe { reciever.as_box(self.allocator.clone()) };

            let Ok(_) = self.do_send(&sender, &reciever.data, Some(current_thread.clone())) else {
                continue;
            };

            if reciever.data.is_auto_reque() {
                inner.reciever_queue.push(Box::into_mem_owner(reciever));
            }

            return Ok(());
        }
    }

    pub fn async_call(&self, listener: EventPoolListenerRef, send_buffer: &UserspaceBuffer, cspace: &Arc<CapabilitySpace>) -> KResult<()> {
        let EventPoolListenerRef {
            event_pool,
            event_id,
        } = listener;

        let sender = ChannelSenderRef {
            cspace: Arc::downgrade(cspace),
            send_buffer: send_buffer.downgrade(),
            inner: ChannelSenderInner::CallEventPool {
                event_pool,
                event_id,
            },
        };

        let mut inner = self.inner();

        loop {
            let Some(reciever) = inner.reciever_queue.pop_front() else {
                let sender = MemOwner::new(sender.into(), &mut self.allocator.clone())?;
                inner.sender_queue.push(sender);

                return Ok(());
            };
            let reciever = unsafe { reciever.as_box(self.allocator.clone()) };

            let Ok(_) = self.do_send(&sender, &reciever.data, None) else {
                continue;
            };

            if reciever.data.is_auto_reque() {
                inner.reciever_queue.push(Box::into_mem_owner(reciever));
            }

            return Ok(());
        }
    }

    pub fn do_send(&self, sender: &ChannelSenderRef, reciever: &ChannelRecieverRef, current_thread_future_ref: Option<ThreadRef>) -> KResult<RecieveResult> {
        let sender_cspace = sender.cspace().ok_or(SysErr::InvlWeak)?;
        let reciever_cspace = reciever.cspace().ok_or(SysErr::InvlWeak)?;

        let send_buffer = sender.send_buffer().ok_or(SysErr::InvlWeak)?;

        let reply_id = if let Some(reply) = sender.get_reply(current_thread_future_ref) {
            let reply = StrongCapability::new_flags(
                Arc::new(
                    reply,
                    self.allocator.clone(),
                )?,
                CapFlags::WRITE,
            );

            let reply_id = reciever_cspace.insert_reply_invisible(Capability::Strong(reply))?;
            Some(reply_id)
        } else {
            None
        };

        let make_reply_visible = || {
            if let Some(reply_id) = reply_id {
                // panic safety: this was inserted earlier, it should be present in reciever cspace
                reciever_cspace.make_reply_visible(reply_id).unwrap();
            }
        };

        let cap_transfer_info = CapabilityTransferInfo {
            src_cspace: &sender_cspace,
            dst_cspace: &reciever_cspace,
        };

        let write_size: KResult<Size> = try {
            match reciever {
                ChannelRecieverRef::Thread { thread, message_buffer, .. } => {
                    let recieve_buffer = message_buffer.upgrade().ok_or(SysErr::InvlWeak)?;
                    if let Some(thread) = thread {
                        let thread = thread.get_thread_as_ready().ok_or(SysErr::OkUnreach)?;

                        let write_size = recieve_buffer.copy_channel_message_from_buffer(&send_buffer, cap_transfer_info)?;
                        thread.set_wake_reason(WakeReason::MsgRecv(RecieveResult {
                            recieve_size: write_size,
                            reply_cap_id: reply_id,
                        }));
    
                        make_reply_visible();
    
                        // FIXME: don't have oom here
                        thread_map().insert_ready_thread(Arc::downgrade(&thread))
                            .expect("failed to insert thread into ready list");
    
                        write_size
                    } else {
                        let write_size = recieve_buffer.copy_channel_message_from_buffer(&send_buffer, cap_transfer_info)?;

                        make_reply_visible();

                        write_size
                    }
                },
                ChannelRecieverRef::EventPool { event_pool, event_id, .. } => {
                    let event_pool = event_pool.upgrade().ok_or(SysErr::InvlWeak)?;

                    let write_size = event_pool.write_channel_event(
                        *event_id,
                        reply_id,
                        &send_buffer,
                        cap_transfer_info,
                    )?;

                    make_reply_visible();

                    // FIXME: handle oom here
                    // no just assume it will be woken later since event is in event pool memory, and oom will be figured out
                    let _ = event_pool.wake_listener();

                    write_size
                },
            }
        };

        match write_size {
            Ok(write_size) => {
                // ignore errors, there is no where to report them to
                let _ = sender.acknowledge_send(write_size);

                Ok(RecieveResult {
                    recieve_size: write_size,
                    reply_cap_id: reply_id,
                })
            },
            Err(error) => {
                if let Some(reply_id) = reply_id {
                    // panic safety: this was inserted earlier, it should be present in reciever cspace
                    reciever_cspace.remove_reply(reply_id).unwrap();
                }

                Err(error)
            },
        }
    }
}

impl Drop for Channel {
    fn drop(&mut self) {
        let inner = self.inner.get_mut();

        while let Some(sender) = inner.sender_queue.pop() {
            unsafe {
                sender.drop_in_place(&mut self.allocator);
            }
        }

        while let Some(reciever) = inner.reciever_queue.pop() {
            unsafe {
                reciever.drop_in_place(&mut self.allocator);
            }
        }
    }
}

impl CapObject for Channel {
    const TYPE: CapType = CapType::Channel;
}

#[derive(Debug, Default)]
struct ChannelInner {
    sender_queue: LinkedList<DefaultNode<ChannelSenderRef>>,
    reciever_queue: LinkedList<DefaultNode<ChannelRecieverRef>>,
}