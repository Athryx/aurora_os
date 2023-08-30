use sys::CapFlags;

use crate::alloc::HeapRef;
use crate::cap::{Capability, StrongCapability};
use crate::cap::drop_check::drop_check_pair;
use crate::prelude::*;
use crate::arch::x64::IntDisable;
use super::options_weak_autodestroy;

pub fn drop_check_new(options: u32, allocator_id: usize, data: usize) -> KResult<(usize, usize)> {
    let weak_auto_destroy = options_weak_autodestroy(options);
    let flags = CapFlags::from_bits_truncate(get_bits(options as usize, 0..4));

    let _int_disable = IntDisable::new();

    let current_process = cpu_local_data().current_process();

    let allocator = current_process.cap_map()
        .get_allocator_with_perms(allocator_id, CapFlags::PROD, weak_auto_destroy)?
        .into_inner();
    let alloc_ref = HeapRef::from_arc(allocator);

    let (drop_check, reciever) = drop_check_pair(data, alloc_ref)?;

    let drop_check = StrongCapability::new_flags(drop_check, flags);
    let reciever = StrongCapability::new_flags(reciever, flags);

    let drop_check_id = current_process.cap_map()
        .insert_drop_check(Capability::Strong(drop_check))?;

    let reciever_id = match current_process.cap_map()
        .insert_drop_check_reciever(Capability::Strong(reciever)) {
            Ok(cap_id) => cap_id,
            Err(error) => {
                // remove drop check id if inserting the reciever failed
                // panic safety: this was just inserted
                current_process.cap_map()
                    .remove_drop_check(drop_check_id)
                    .unwrap();

                return Err(error);
            }
        };

    Ok((drop_check_id.into(), reciever_id.into()))
}