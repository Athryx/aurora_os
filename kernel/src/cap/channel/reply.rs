use core::sync::atomic::{AtomicBool, Ordering};

use sys::CapType;

use crate::prelude::*;
use crate::cap::{CapObject, capability_space::CapabilitySpace};
use crate::event::UserspaceBuffer;
use crate::sched::{thread_map, WakeReason};
use crate::container::Arc;

use super::{CapabilityTransferInfo, RecieveResult};
use super::event_listeners::ChannelRecieverRef;

#[derive(Debug)]
pub struct Reply {
    listener: ChannelRecieverRef,
    reply_fired: AtomicBool,
}

impl Reply {
    pub fn new(listener: ChannelRecieverRef) -> Self {
        Reply {
            listener,
            reply_fired: AtomicBool::new(false),
        }
    }

    pub fn reply(&self, src_buffer: &UserspaceBuffer, src_cspace: &CapabilitySpace) -> KResult<Size> {
        // this only need relaxed ordering, since the only guarentee we need is max 1 thread runs reply
        // other synchronizing of memory will occur insice of listener
        if self.reply_fired.swap(true, Ordering::Relaxed) {
            self.reply_inner(src_buffer, src_cspace)
        } else {
            // this reply has already been replied to
            Err(SysErr::InvlOp)
        }
    }

    fn reply_inner(&self, src_buffer: &UserspaceBuffer, src_cspace: &CapabilitySpace) -> KResult<Size> {
        match &self.listener {
            ChannelRecieverRef::Thread {
                thread,
                message_buffer,
                cspace,
            } => {
                let dst_cspace = cspace.upgrade().ok_or(SysErr::InvlWeak)?;
                let dst_buffer = message_buffer.upgrade().ok_or(SysErr::InvlWeak)?;

                let thread = thread.as_ref().expect("reply must have a valid listening thread");
                let thread = thread.get_thread_as_ready().ok_or(SysErr::OkUnreach)?;

                let write_size = dst_buffer.copy_channel_message_from_buffer(src_buffer, CapabilityTransferInfo {
                    src_cspace,
                    dst_cspace: &dst_cspace,
                })?;

                thread.set_wake_reason(WakeReason::MsgRecv(RecieveResult {
                    recieve_size: write_size,
                    reply_cap_id: None,
                }));

                // FIXME: don't have oom here
                thread_map().insert_ready_thread(Arc::downgrade(&thread))
                    .expect("failed to insert thread into ready list");

                Ok(write_size)
            },
            ChannelRecieverRef::EventPool {
                event_pool,
                event_id,
                cspace,
                ..
            } => {
                let dst_cspace = cspace.upgrade().ok_or(SysErr::InvlWeak)?;
                let event_pool = event_pool.upgrade().ok_or(SysErr::InvlWeak)?;

                let write_size = event_pool.write_channel_event(
                    *event_id,
                    None,
                    src_buffer,
                    CapabilityTransferInfo {
                        src_cspace,
                        dst_cspace: &dst_cspace,
                    },
                )?;

                event_pool.wake_listener()?;

                Ok(write_size)
            },
        }
    }
}

impl CapObject for Reply {
    const TYPE: CapType = CapType::Reply;
}