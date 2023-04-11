use crate::alloc::OrigRef;
use crate::prelude::*;
use crate::cap::{CapFlags, StrongCapability, Capability};
use crate::arch::x64::IntDisable;
use crate::process::{Spawner, Process};
use super::options_weak_autodestroy;

/// Creates a new spawner
/// 
/// `spawn_key` is a key passed to userspace during boot that allows creation of spawners
///
/// # Options
/// bit 0-3 (spawner_cap_flags): CapPriv representing privalidges over this spawner
///
/// # Required Capability Permissions
/// `allocator`: cap_prod
/// `spawn_key`: cap_read
/// 
/// # Syserr Code
/// InvlArgs: `spawn_key` is not the correct spawn key
///
/// # Returns
/// spawner: capability to a new spawner object
pub fn spawner_new(
    options: u32,
    allocator_id: usize,
    spawn_key_id: usize
) -> KResult<usize> {
    let weak_auto_destroy = options_weak_autodestroy(options);
    let spawner_cap_flags = CapFlags::from_bits_truncate(get_bits(options as usize, 0..4));

    let _int_disable = IntDisable::new();

    let current_process = cpu_local_data().current_process();

    let allocator = current_process.cap_map()
        .get_allocator_with_perms(allocator_id, CapFlags::PROD, weak_auto_destroy)?
        .into_inner();
    let alloc_ref = OrigRef::from_arc(allocator);

    let spawn_key = current_process.cap_map()
        .get_key_with_perms(spawn_key_id, CapFlags::READ, weak_auto_destroy)?
        .into_inner();

    if Spawner::key_id() != spawn_key.id() {
        return Err(SysErr::InvlArgs);
    }

    let spawner = StrongCapability::new_flags(
        Spawner::new(alloc_ref.downgrade()),
        spawner_cap_flags,
        alloc_ref,
    )?;

    Ok(current_process.cap_map().insert_spawner(Capability::Strong(spawner))?.into())
}

/// kills all the processes that were made with this spawner
///
/// # Required Capability Permissions
/// `spawner`: cap_write
pub fn spawner_kill_all(options: u32, spawner_id: usize) -> KResult<()> {
    let weak_auto_destroy = options_weak_autodestroy(options);

    let _int_disable = IntDisable::new();

    let current_process = cpu_local_data()
        .current_process()
        .cap_map()
        .get_spawner_with_perms(spawner_id, CapFlags::WRITE, weak_auto_destroy)?
        .into_inner()
        .kill_all_processes();

    if let Some(current_process) = current_process {
        // at this point no other resources are held so it is safe to exit the current process
        Process::exit(current_process);
    }

    Ok(())
}