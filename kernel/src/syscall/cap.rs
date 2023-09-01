use sys::{KResult, CapId, SysErr, CapCloneFlags, CapFlags, CapType, CapDestroyFlags};

use crate::{arch::x64::IntDisable, prelude::cpu_local_data, cap::CapabilityMap};

use super::options_weak_autodestroy;

pub fn cap_clone(
    options: u32,
    dst_process_id: usize,
    src_process_id: usize,
    cap_id: usize,
) -> KResult<usize> {
    let weak_auto_destroy = options_weak_autodestroy(options);
    let flags = CapCloneFlags::from_bits_truncate(options);
    let new_cap_perms = CapFlags::from(flags);

    let old_cap = CapId::try_from(cap_id)
        .ok_or(SysErr::InvlId)?;

    let _int_disable = IntDisable::new();

    let src_process = if flags.contains(CapCloneFlags::SRC_PROCESS_SELF) {
        cpu_local_data().current_process()
    } else {
        cpu_local_data().current_process().cap_map()
            .get_process_with_perms(src_process_id, CapFlags::WRITE, weak_auto_destroy)?
            .into_inner()
    };
    let src_cap_map = src_process.cap_map();

    let dst_process = if flags.contains(CapCloneFlags::DST_PROCESS_SELF) {
        cpu_local_data().current_process()
    } else {
        cpu_local_data().current_process().cap_map()
            .get_process_with_perms(dst_process_id, CapFlags::WRITE, weak_auto_destroy)?
            .into_inner()
    };
    let dst_cap_map = dst_process.cap_map();

    macro_rules! call_cap_clone {
        ($cap_map_clone:ident) => {
            CapabilityMap::$cap_map_clone(
                dst_cap_map,
                src_cap_map,
                old_cap,
                new_cap_perms,
                !flags.contains(CapCloneFlags::MAKE_WEAK),
                flags.contains(CapCloneFlags::DESTROY_SRC_CAP),
                weak_auto_destroy,
            )?
        };
    }

    let new_cap_id = match old_cap.cap_type() {
        CapType::Process => call_cap_clone!(clone_process),
        CapType::Memory => call_cap_clone!(clone_memory),
        //CapType::Lock => call_cap_clone!(clone_),
        //CapType::BoundedEventPool => call_cap_clone!(clone_),
        //CapType::UnboundedEventPool => call_cap_clone!(clone_),
        CapType::Channel => call_cap_clone!(clone_channel),
        //CapType::MessageCapacity => call_cap_clone!(clone_),
        CapType::Key => call_cap_clone!(clone_key),
        //CapType::Interrupt => call_cap_clone!(clone_),
        //CapType::Port => call_cap_clone!(clone_),
        CapType::Spawner => call_cap_clone!(clone_spawner),
        CapType::Allocator => call_cap_clone!(clone_allocator),
        CapType::DropCheck => call_cap_clone!(clone_drop_check),
        CapType::DropCheckReciever => call_cap_clone!(clone_drop_check_reciever),
        //CapType::RootOom => call_cap_clone!(clone_),
        //CapType::MmioAllocator => call_cap_clone!(clone_),
        //CapType::IntAllocator => call_cap_clone!(clone_),
        //CapType::PortAllocator => call_cap_clone!(clone_),
        _ => todo!(),
    };

    Ok(new_cap_id.into())
}

pub fn cap_destroy(
    options: u32,
    process_id: usize,
    cap_id: usize,
) -> KResult<()> {
    let weak_auto_destroy = options_weak_autodestroy(options);
    let flags = CapDestroyFlags::from_bits_truncate(options);

    let cap_id = CapId::try_from(cap_id)
        .ok_or(SysErr::InvlId)?;

    let _int_disable = IntDisable::new();

    let process = if flags.contains(CapDestroyFlags::PROCESS_SELF) {
        cpu_local_data().current_process()
    } else {
        cpu_local_data().current_process().cap_map()
            .get_process_with_perms(process_id, CapFlags::WRITE, weak_auto_destroy)?
            .into_inner()
    };
    let cap_map = process.cap_map();

    match cap_id.cap_type() {
        CapType::Process => { cap_map.remove_process(cap_id)?; },
        CapType::Memory => { cap_map.remove_memory(cap_id)?; },
        //CapType::Lock => call_cap_clone!(clone_),
        //CapType::BoundedEventPool => call_cap_clone!(clone_),
        //CapType::UnboundedEventPool => call_cap_clone!(clone_),
        CapType::Channel => { cap_map.remove_channel(cap_id)?; },
        //CapType::MessageCapacity => call_cap_clone!(clone_),
        CapType::Key => { cap_map.remove_key(cap_id)?; },
        //CapType::Interrupt => call_cap_clone!(clone_),
        //CapType::Port => call_cap_clone!(clone_),
        CapType::Spawner => { cap_map.remove_spawner(cap_id)?; },
        CapType::Allocator => { cap_map.remove_allocator(cap_id)?; },
        CapType::DropCheck => { cap_map.remove_drop_check(cap_id)?; },
        CapType::DropCheckReciever => { cap_map.remove_drop_check_reciever(cap_id)?; },
        //CapType::RootOom => call_cap_clone!(clone_),
        //CapType::MmioAllocator => call_cap_clone!(clone_),
        //CapType::IntAllocator => call_cap_clone!(clone_),
        //CapType::PortAllocator => call_cap_clone!(clone_),
        _ => todo!(),
    }

    Ok(())
}