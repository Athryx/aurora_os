use sys::{CapFlags, CapId, ChannelSyncFlags};

use crate::alloc::HeapRef;
use crate::cap::channel::SendRecvResult;
use crate::cap::{Capability, StrongCapability, channel::Channel};
use crate::container::Arc;
use crate::event::UserspaceBuffer;
use crate::prelude::*;
use crate::arch::x64::IntDisable;
use crate::sched::{switch_current_thread_to, ThreadState, PostSwitchAction};

use super::options_weak_autodestroy;

pub fn channel_new(options: u32, allocator_id: usize) -> KResult<usize> {
    let weak_auto_destroy = options_weak_autodestroy(options);
    let channel_cap_flags = CapFlags::from_bits_truncate(get_bits(options as usize, 0..4));

    let _int_disable = IntDisable::new();

    let current_process = cpu_local_data().current_process();

    let allocator = current_process.cap_map()
        .get_allocator_with_perms(allocator_id, CapFlags::PROD, weak_auto_destroy)?
        .into_inner();
    let heap_ref = HeapRef::from_arc(allocator);

    let channel = StrongCapability::new_flags(
        Arc::new(Channel::new(heap_ref.clone()), heap_ref)?,
        channel_cap_flags,
    );

    Ok(current_process.cap_map().insert_channel(Capability::Strong(channel))?.into())
}

/// Used for `channel_try_send`, `channel_sync_send`, `channel_try_recv`, `channel_sync_recv` to process common arguments
fn channel_handle_args(
    options: u32,
    channel_id: usize,
    msg_buf_id: usize,
    msg_buf_offset: usize,
    msg_buf_size: usize,
    msg_buf_perms: CapFlags,
) -> KResult<(Arc<Channel>, UserspaceBuffer)> {
    let weak_auto_destroy = options_weak_autodestroy(options);

    let current_process = cpu_local_data().current_process();

    let channel = current_process.cap_map()
        .get_channel_with_perms(channel_id, CapFlags::PROD, weak_auto_destroy)?
        .into_inner();

    let buffer = current_process.cap_map()
        .get_userspace_buffer(
            msg_buf_id,
            msg_buf_offset,
            msg_buf_size,
            msg_buf_perms,
            weak_auto_destroy,
        )?;
    
    Ok((channel, buffer))
}

pub fn channel_try_send(
    options: u32,
    channel_id: usize,
    msg_buf_id: usize,
    msg_buf_offset: usize,
    msg_buf_size: usize,
) -> KResult<usize> {
    let _int_disable = IntDisable::new();

    let (channel, buffer) = channel_handle_args(
        options,
        channel_id,
        msg_buf_id,
        msg_buf_offset,
        msg_buf_size,
        CapFlags::READ,
    )?;

    channel.try_send(&buffer)
}

pub fn channel_sync_send(
    options: u32,
    channel_id: usize,
    msg_buf_id: usize,
    msg_buf_offset: usize,
    msg_buf_size: usize,
    timeout: usize,
) -> KResult<usize> {
    let flags = ChannelSyncFlags::from_bits_truncate(options);

    let int_disable = IntDisable::new();

    let (channel, buffer) = channel_handle_args(
        options,
        channel_id,
        msg_buf_id,
        msg_buf_offset,
        msg_buf_size,
        CapFlags::READ,
    )?;

    match channel.sync_send(buffer) {
        SendRecvResult::Success(write_size) => Ok(write_size),
        SendRecvResult::Error(error) => Err(error),
        SendRecvResult::Block => {
            drop(channel);

            let post_switch_hook = if flags.contains(ChannelSyncFlags::TIMEOUT) {
                PostSwitchAction::SetTimeout(timeout as u64)
            } else {
                PostSwitchAction::None
            };

            switch_current_thread_to(
                ThreadState::Suspended,
                int_disable,
                post_switch_hook,
                false,
            ).expect("failed to suspend thread while waiting on channel");

            // FIXME: report write size correctly
            Ok(0)
        },
    }
}

pub fn channel_try_recv(
    options: u32,
    channel_id: usize,
    msg_buf_id: usize,
    msg_buf_offset: usize,
    msg_buf_size: usize,
) -> KResult<usize> {
    let _int_disable = IntDisable::new();

    let (channel, buffer) = channel_handle_args(
        options,
        channel_id,
        msg_buf_id,
        msg_buf_offset,
        msg_buf_size,
        CapFlags::WRITE,
    )?;
    
    channel.try_recv(&buffer)
}

pub fn channel_sync_recv(
    options: u32,
    channel_id: usize,
    msg_buf_id: usize,
    msg_buf_offset: usize,
    msg_buf_size: usize,
    timeout: usize,
) -> KResult<usize> {
    let flags = ChannelSyncFlags::from_bits_truncate(options);

    let int_disable = IntDisable::new();

    let (channel, buffer) = channel_handle_args(
        options,
        channel_id,
        msg_buf_id,
        msg_buf_offset,
        msg_buf_size,
        CapFlags::WRITE,
    )?;

    match channel.sync_recv(buffer) {
        SendRecvResult::Success(write_size) => Ok(write_size),
        SendRecvResult::Error(error) => Err(error),
        SendRecvResult::Block => {
            drop(channel);

            let post_switch_hook = if flags.contains(ChannelSyncFlags::TIMEOUT) {
                PostSwitchAction::SetTimeout(timeout as u64)
            } else {
                PostSwitchAction::None
            };

            switch_current_thread_to(
                ThreadState::Suspended,
                int_disable,
                post_switch_hook,
                false,
            ).expect("failed to suspend thread while waiting on channel");

            // FIXME: report write size correctly
            Ok(0)
        },
    }
}