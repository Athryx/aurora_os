use sys::CapFlags;

use crate::alloc::HeapRef;
use crate::cap::{Capability, StrongCapability, channel::Channel};
use crate::container::Arc;
use crate::prelude::*;
use crate::arch::x64::IntDisable;

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
        Arc::new(Channel::new(), heap_ref)?,
        channel_cap_flags,
    );

    Ok(current_process.cap_map().insert_channel(Capability::Strong(channel))?.into())
}