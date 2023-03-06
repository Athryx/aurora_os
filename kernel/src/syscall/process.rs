use crate::{prelude::*, cap::{CapFlags, Capability}, arch::x64::IntDisable, alloc::{PaRef, OrigRef}};
use crate::process::Process;
use super::options_weak_autodestroy;

/// creates a new process with name `name`
/// 
/// in order to avoid memory leaks due to reference cycles, process_new always returns an unupgradable weak capability
/// the kernel keeps 1 internal strong refernce to each process when it is created
/// in order to destroy the process, call process_exit to destroy the strong refernce to the process, which will dealloc the process
/// the process is not freed when all weak references are destroyed
///
/// # Options
/// bits 0-2 (process_cap_flags): CapPriv representing read, prod, and write privalidges of new capability
///
/// # Required Capability Permissions:
/// `allocator`: cap_prod
/// `spawner`: cap_prod
///
/// # Returns
/// pocess: capability of new process
// TODO: process name
pub fn process_new(options: u32, allocator_id: usize, spawner_id: usize) -> KResult<usize> {
    let weak_auto_destroy = options_weak_autodestroy(options);

    let _int_disable = IntDisable::new();

    let process = cpu_local_data().current_process();

    let allocator = process.cap_map()
        .get_allocator_with_perms(allocator_id, CapFlags::PROD, weak_auto_destroy)?;

    let spawner = process.cap_map()
        .get_spawner_with_perms(spawner_id, CapFlags::PROD, weak_auto_destroy)?;

    let page_allocator = PaRef::from_arc(allocator.clone());
    let heap_allocator = OrigRef::from_arc(allocator);

    // TODO: process name
    let name = String::new(heap_allocator.downgrade());
    let new_process = Process::new(
        page_allocator,
        heap_allocator,
        name,
    )?;

    spawner.add_process(new_process.inner().clone())?;

    Ok(process.cap_map().insert_process(Capability::Weak(new_process))?.into())
}