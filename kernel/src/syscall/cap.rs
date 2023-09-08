use sys::{KResult, CapId, SysErr, CapCloneFlags, CapFlags, CapType, CapDestroyFlags};

use crate::cap::capability_space::CapCloneWeakness;
use crate::prelude::*;
use crate::{arch::x64::IntDisable, cap::capability_space::CapabilitySpace};

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

    let cap_weakness = if flags.contains(CapCloneFlags::CHANGE_CAP_WEAKNESS) {
        if flags.contains(CapCloneFlags::MAKE_WEAK) {
            CapCloneWeakness::MakeWeak
        } else {
            CapCloneWeakness::MakeStrong
        }
    } else {
        CapCloneWeakness::KeepSame
    };

    let old_cap = CapId::try_from(cap_id)
        .ok_or(SysErr::InvlId)?;

    let _int_disable = IntDisable::new();

    let src_cspace = if flags.contains(CapCloneFlags::SRC_CSPACE_SELF) {
        CapabilitySpace::current()
    } else {
        CapabilitySpace::current()
            .get_capability_space_with_perms(src_process_id, CapFlags::WRITE, weak_auto_destroy)?
            .into_inner()
    };

    let dst_cspace = if flags.contains(CapCloneFlags::DST_CSPACE_SELF) {
        CapabilitySpace::current()
    } else {
        CapabilitySpace::current()
            .get_capability_space_with_perms(dst_process_id, CapFlags::WRITE, weak_auto_destroy)?
            .into_inner()
    };

    macro_rules! call_cap_clone {
        ($cspace_clone:ident) => {
            CapabilitySpace::$cspace_clone(
                &dst_cspace,
                &src_cspace,
                old_cap,
                new_cap_perms,
                cap_weakness,
                flags.contains(CapCloneFlags::DESTROY_SRC_CAP),
                weak_auto_destroy,
            )?
        };
    }

    let new_cap_id = match old_cap.cap_type() {
        CapType::Thread => call_cap_clone!(clone_thread),
        CapType::ThreadGroup => call_cap_clone!(clone_thread_group),
        CapType::AddressSpace => call_cap_clone!(clone_address_space),
        CapType::CapabilitySpace => call_cap_clone!(clone_capability_space),
        CapType::Memory => call_cap_clone!(clone_memory),
        //CapType::Lock => call_cap_clone!(clone_),
        //CapType::BoundedEventPool => call_cap_clone!(clone_),
        //CapType::UnboundedEventPool => call_cap_clone!(clone_),
        CapType::Channel => call_cap_clone!(clone_channel),
        //CapType::MessageCapacity => call_cap_clone!(clone_),
        CapType::Key => call_cap_clone!(clone_key),
        //CapType::Interrupt => call_cap_clone!(clone_),
        //CapType::Port => call_cap_clone!(clone_),
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

    let cspace = if flags.contains(CapDestroyFlags::CSPACE_SELF) {
        CapabilitySpace::current()
    } else {
        CapabilitySpace::current()
            .get_capability_space_with_perms(process_id, CapFlags::WRITE, weak_auto_destroy)?
            .into_inner()
    };

    match cap_id.cap_type() {
        CapType::Thread => { cspace.remove_thread(cap_id)?; },
        CapType::ThreadGroup => { cspace.remove_thread_group(cap_id)?; },
        CapType::AddressSpace => { cspace.remove_address_space(cap_id)?; },
        CapType::CapabilitySpace => { cspace.remove_capability_space(cap_id)?; },
        CapType::Memory => { cspace.remove_memory(cap_id)?; },
        //CapType::Lock => call_cap_clone!(clone_),
        //CapType::BoundedEventPool => call_cap_clone!(clone_),
        //CapType::UnboundedEventPool => call_cap_clone!(clone_),
        CapType::Channel => { cspace.remove_channel(cap_id)?; },
        //CapType::MessageCapacity => call_cap_clone!(clone_),
        CapType::Key => { cspace.remove_key(cap_id)?; },
        //CapType::Interrupt => call_cap_clone!(clone_),
        //CapType::Port => call_cap_clone!(clone_),
        CapType::Allocator => { cspace.remove_allocator(cap_id)?; },
        CapType::DropCheck => { cspace.remove_drop_check(cap_id)?; },
        CapType::DropCheckReciever => { cspace.remove_drop_check_reciever(cap_id)?; },
        //CapType::RootOom => call_cap_clone!(clone_),
        //CapType::MmioAllocator => call_cap_clone!(clone_),
        //CapType::IntAllocator => call_cap_clone!(clone_),
        //CapType::PortAllocator => call_cap_clone!(clone_),
        _ => todo!(),
    }

    Ok(())
}