use sys::{CapFlags, ChannelSyncFlags, ChannelAsyncRecvFlags, EventId};

use crate::alloc::HeapRef;
use crate::cap::capability_space::CapabilitySpace;
use crate::cap::channel::SendRecvResult;
use crate::cap::{Capability, StrongCapability, channel::Channel};
use crate::container::Arc;
use crate::event::{UserspaceBuffer, EventPoolListenerRef};
use crate::prelude::*;
use crate::arch::x64::IntDisable;
use crate::sched::{switch_current_thread_to, ThreadState, PostSwitchAction, WakeReason};

use super::options_weak_autodestroy;

pub fn channel_new(options: u32, allocator_id: usize) -> KResult<usize> {
    let weak_auto_destroy = options_weak_autodestroy(options);
    let channel_cap_flags = CapFlags::from_bits_truncate(get_bits(options as usize, 0..4));

    let _int_disable = IntDisable::new();

    let cspace = CapabilitySpace::current();

    let allocator = cspace
        .get_allocator_with_perms(allocator_id, CapFlags::PROD, weak_auto_destroy)?
        .into_inner();
    let heap_ref = HeapRef::from_arc(allocator);

    let channel = StrongCapability::new_flags(
        Arc::new(Channel::new(heap_ref.clone()), heap_ref)?,
        channel_cap_flags,
    );

    Ok(cspace.insert_channel(Capability::Strong(channel))?.into())
}

/// Used for `channel_try_send`, `channel_sync_send`, `channel_try_recv`, `channel_sync_recv` to process common arguments
fn channel_handle_args(
    options: u32,
    channel_id: usize,
    channel_perms: CapFlags,
    msg_buf_id: usize,
    msg_buf_offset: usize,
    msg_buf_size: usize,
    msg_buf_perms: CapFlags,
) -> KResult<(Arc<Channel>, UserspaceBuffer, Arc<CapabilitySpace>)> {
    let weak_auto_destroy = options_weak_autodestroy(options);

    let cspace = CapabilitySpace::current();

    let channel = cspace
        .get_channel_with_perms(channel_id, channel_perms, weak_auto_destroy)?
        .into_inner();

    let buffer = cspace
        .get_userspace_buffer(
            msg_buf_id,
            msg_buf_offset,
            msg_buf_size,
            msg_buf_perms,
            weak_auto_destroy,
        )?;
    
    Ok((channel, buffer, cspace))
}

pub fn channel_try_send(
    options: u32,
    channel_id: usize,
    msg_buf_id: usize,
    msg_buf_offset: usize,
    msg_buf_size: usize,
) -> KResult<usize> {
    let _int_disable = IntDisable::new();

    let (channel, buffer, cspace) = channel_handle_args(
        options,
        channel_id,
        CapFlags::PROD,
        msg_buf_id,
        msg_buf_offset,
        msg_buf_size,
        CapFlags::READ,
    )?;

    channel.try_send(&buffer, &cspace).map(Size::bytes)
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

    let (channel, buffer, cspace) = channel_handle_args(
        options,
        channel_id,
        CapFlags::PROD,
        msg_buf_id,
        msg_buf_offset,
        msg_buf_size,
        CapFlags::READ,
    )?;

    match channel.sync_send(buffer, cspace) {
        SendRecvResult::Success(write_size) => Ok(write_size.bytes()),
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

            let _int_disable = IntDisable::new();
            match cpu_local_data().current_thread().wake_reason() {
                WakeReason::MsgSendRecv { msg_size } => Ok(msg_size.bytes()),
                WakeReason::Timeout => Err(SysErr::OkTimeout),
                _ => unreachable!(),
            }
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

    let (channel, buffer, cspace) = channel_handle_args(
        options,
        channel_id,
        CapFlags::WRITE,
        msg_buf_id,
        msg_buf_offset,
        msg_buf_size,
        CapFlags::WRITE,
    )?;
    
    channel.try_recv(&buffer, &cspace).map(Size::bytes)
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

    let (channel, buffer, cspace) = channel_handle_args(
        options,
        channel_id,
        CapFlags::WRITE,
        msg_buf_id,
        msg_buf_offset,
        msg_buf_size,
        CapFlags::WRITE,
    )?;

    match channel.sync_recv(buffer, cspace) {
        SendRecvResult::Success(write_size) => Ok(write_size.bytes()),
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

            let _int_disable = IntDisable::new();
            match cpu_local_data().current_thread().wake_reason() {
                WakeReason::MsgSendRecv { msg_size } => Ok(msg_size.bytes()),
                WakeReason::Timeout => Err(SysErr::OkTimeout),
                _ => unreachable!(),
            }
        },
    }
}

pub fn channel_async_send(
    options: u32,
    channel_id: usize,
    msg_buf_id: usize,
    msg_buf_offset: usize,
    msg_buf_size: usize,
    event_pool_id: usize,
    event_id: usize,
) -> KResult<()> {
    let event_id = EventId::from_u64(event_id as u64);

    let _int_disable = IntDisable::new();

    let (channel, buffer, cspace) = channel_handle_args(
        options,
        channel_id,
        CapFlags::PROD,
        msg_buf_id,
        msg_buf_offset,
        msg_buf_size,
        CapFlags::READ,
    )?;

    let event_pool = CapabilitySpace::current()
        .get_event_pool_with_perms(event_pool_id, CapFlags::WRITE, options_weak_autodestroy(options))?
        .into_inner();

    let event_pool_listener = EventPoolListenerRef {
        event_pool: Arc::downgrade(&event_pool),
        event_id,
    };

    channel.async_send(event_pool_listener, buffer, cspace)
}

pub fn channel_async_recv(
    options: u32,
    channel_id: usize,
    event_pool_id: usize,
    event_id: usize,
) -> KResult<()> {
    let weak_auto_destroy = options_weak_autodestroy(options);
    let flags = ChannelAsyncRecvFlags::from_bits_truncate(options);

    let event_id = EventId::from_u64(event_id as u64);

    let _int_disable = IntDisable::new();

    let cspace = CapabilitySpace::current();

    let channel = cspace
        .get_channel_with_perms(channel_id, CapFlags::WRITE, weak_auto_destroy)?
        .into_inner();

    let event_pool = cspace
        .get_event_pool_with_perms(event_pool_id, CapFlags::WRITE, weak_auto_destroy)?
        .into_inner();

    let event_pool_listener = EventPoolListenerRef {
        event_pool: Arc::downgrade(&event_pool),
        event_id,
    };

    channel.async_recv(event_pool_listener, flags.contains(ChannelAsyncRecvFlags::AUTO_REQUE), cspace)
}