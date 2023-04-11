use crate::cap::{StrongCapability, Capability};
use crate::prelude::*;
use crate::cap::{CapFlags, key::Key};
use crate::{arch::x64::IntDisable, alloc::OrigRef};
use super::options_weak_autodestroy;

/// Ceates a new key object
/// 
/// keys are used by userpace servers to identify sessions, and manage permissions
/// each key is a globally unique integer, and this integer can be obtained with the key_id syscall
///
/// # Options
/// bits 0-3 (key_cap_flags): specifies the permissions of the returned key capability
///
/// Required Capability Permissions
/// `allocator`: cap_prod
///
/// # Returns
/// key: key capability id
pub fn key_new(options: u32, allocator_id: usize) -> KResult<usize> {
    let weak_auto_destroy = options_weak_autodestroy(options);
    let key_cap_flags = CapFlags::from_bits_truncate(get_bits(options as usize, 0..4));

    let _int_disable = IntDisable::new();

    let current_process = cpu_local_data().current_process();

    let allocator = current_process.cap_map()
        .get_allocator_with_perms(allocator_id, CapFlags::PROD, weak_auto_destroy)?
        .into_inner();
    let alloc_ref = OrigRef::from_arc(allocator);

    let key = StrongCapability::new_flags(
        Key::new(),
        key_cap_flags,
        alloc_ref,
    )?;

    Ok(current_process.cap_map().insert_key(Capability::Strong(key))?.into())
}

/// returns `key`s id
///
/// # Required Capability Permissions
/// `key`: cap_read
///
/// # Returns
/// id: the key's id
pub fn key_id(options: u32, key_cap_id: usize) -> KResult<usize> {
    let weak_auto_destroy = options_weak_autodestroy(options);

    let _int_disable = IntDisable::new();

    Ok(cpu_local_data()
        .current_process()
        .cap_map()
        .get_key_with_perms(key_cap_id, CapFlags::READ, weak_auto_destroy)?
        .into_inner()
        .id() as usize)
}