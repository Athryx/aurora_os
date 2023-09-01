use core::sync::atomic::{AtomicUsize, Ordering};

use paste::paste;

use crate::event::UserspaceBuffer;
use crate::{prelude::*, alloc::HeapRef};
use crate::container::HashMap;
use crate::process::{Process, Spawner};
use crate::alloc::CapAllocator;
use crate::sync::IMutex;

use super::drop_check::{DropCheck, DropCheckReciever};
use super::{CapId, Capability, StrongCapability, CapFlags, CapObject, key::Key, memory::Memory, channel::Channel};

type InnerCapMap<T> = IMutex<HashMap<CapId, Capability<T>>>;

/// A map that holds all the capabilities in a process
#[derive(Debug)]
pub struct CapabilityMap {
    next_id: AtomicUsize,
    process_map: InnerCapMap<Process>,
    memory_map: InnerCapMap<Memory>,
    key_map: InnerCapMap<Key>,
    channel_map: InnerCapMap<Channel>,
    spawner_map: InnerCapMap<Spawner>,
    allocator_map: InnerCapMap<CapAllocator>,
    drop_check_map: InnerCapMap<DropCheck>,
    drop_check_reciever_map: InnerCapMap<DropCheckReciever>,
}

impl CapabilityMap {
    pub fn new(allocator: HeapRef) -> Self {
        CapabilityMap {
            next_id: AtomicUsize::new(0),
            process_map: IMutex::new(HashMap::new(allocator.clone())),
            memory_map: IMutex::new(HashMap::new(allocator.clone())),
            key_map: IMutex::new(HashMap::new(allocator.clone())),
            channel_map: IMutex::new(HashMap::new(allocator.clone())),
            spawner_map: IMutex::new(HashMap::new(allocator.clone())),
            allocator_map: IMutex::new(HashMap::new(allocator.clone())),
            drop_check_map: IMutex::new(HashMap::new(allocator.clone())),
            drop_check_reciever_map: IMutex::new(HashMap::new(allocator)),
        }
    }
}

macro_rules! generate_cap_methods {
    ($map:ty, $cap_type:ty, $cap_map:ident, $cap_name:ident) => {
        paste! {
            impl $map {
                pub fn [<insert_ $cap_name>](&self, mut capability: Capability<$cap_type>) -> KResult<CapId> {
                    let next_id = self.next_id.fetch_add(1, Ordering::Relaxed);
                    
                    let cap_id = CapId::new(
                        $cap_type::TYPE,
                        capability.flags(),
                        capability.is_weak(),
                        next_id
                    );

                    capability.set_id(cap_id);

                    self.$cap_map.lock().insert(cap_id, capability)?;
                    Ok(cap_id)
                }

                pub fn [<remove_ $cap_name>](&self, cap_id: CapId) -> KResult<Capability<$cap_type>> {
                    self.$cap_map.lock().remove(&cap_id)
                        .ok_or(SysErr::InvlId)
                }

                pub fn [<get_strong_ $cap_name _with_perms>](
                    &self,
                    cap_id: usize,
                    required_perms: CapFlags,
                ) -> KResult<StrongCapability<$cap_type>> {
                    let map = self.$cap_map.lock();

                    let cap_id = CapId::try_from(cap_id).ok_or(SysErr::InvlId)?;
                    let cap = map.get(&cap_id).ok_or(SysErr::InvlId)?;

                    if !cap.flags().contains(required_perms) {
                        return Err(SysErr::InvlPerm)
                    }

                    match cap {
                        Capability::Strong(cap) => Ok(cap.clone()),
                        Capability::Weak(_) => Err(SysErr::InvlWeak),
                    }
                }

                pub fn [<get_ $cap_name _with_perms>](
                    &self,
                    cap_id: usize,
                    required_perms: CapFlags,
                    weak_auto_destroy: bool,
                ) -> KResult<StrongCapability<$cap_type>> {
                    let mut map = self.$cap_map.lock();

                    let cap_id = CapId::try_from(cap_id).ok_or(SysErr::InvlId)?;
                    let cap = map.get(&cap_id).ok_or(SysErr::InvlId)?;

                    if !cap.flags().contains(required_perms) {
                        return Err(SysErr::InvlPerm)
                    }

                    match cap {
                        Capability::Strong(cap) => Ok(cap.clone()),
                        Capability::Weak(cap) => {
                            let strong = cap.upgrade();

                            match strong {
                                Some(cap) => Ok(cap),
                                None => {
                                    if weak_auto_destroy {
                                        map.remove(&cap_id);
                                    }

                                    Err(SysErr::InvlWeak)
                                }
                            }
                        },
                    }
                }

                pub fn [<get_strong_ $cap_name>](
                    &self,
                    cap_id: usize,
                    weak_auto_destroy: bool,
                ) -> KResult<StrongCapability<$cap_type>> {
                    let mut map = self.$cap_map.lock();

                    let cap_id = CapId::try_from(cap_id).ok_or(SysErr::InvlId)?;
                    let cap = map.get(&cap_id).ok_or(SysErr::InvlId)?;

                    match cap {
                        Capability::Strong(cap) => Ok(cap.clone()),
                        Capability::Weak(cap) => {
                            let strong = cap.upgrade();

                            match strong {
                                Some(cap_strong) => Ok(cap_strong),
                                None => {
                                    if weak_auto_destroy {
                                        map.remove(&cap_id);
                                    }

                                    Err(SysErr::InvlWeak)
                                }
                            }
                        },
                    }
                }

                pub fn [<get_ $cap_name>](&self, cap_id: CapId) -> KResult<Capability<$cap_type>> {
                    let map = self.$cap_map.lock();

                    Ok(map.get(&cap_id).ok_or(SysErr::InvlId)?.clone())
                }

                /// Used by cap_clone syscall
                // TODO: don't have so many arguments
                pub fn [<clone_ $cap_name>](
                    dst: &Self,
                    src: &Self,
                    cap_id: CapId,
                    new_perms: CapFlags,
                    make_strong_cap: bool,
                    destroy_old_cap: bool,
                    weak_auto_destroy: bool,
                ) -> KResult<CapId> {
                    let capability = src.[<get_ $cap_name>](cap_id)?;

                    let new_flags_capid = CapId::null_flags(capability.flags() & new_perms, !make_strong_cap);

                    let new_capability = match capability {
                        Capability::Strong(mut capability) => {
                            capability.id = new_flags_capid;

                            if make_strong_cap {
                                Capability::Strong(capability)
                            } else {
                                Capability::Weak(capability.downgrade())
                            }
                        },
                        Capability::Weak(mut capability) => {
                            capability.id = new_flags_capid;

                            if make_strong_cap {
                                let Some(strong_cap) = capability.upgrade() else {
                                    if weak_auto_destroy {
                                        // panic safety: this capability is already checked to exist
                                        src.[<remove_ $cap_name>](cap_id).unwrap();
                                    }

                                    return Err(SysErr::InvlWeak);
                                };

                                Capability::Strong(strong_cap)
                            } else {
                                Capability::Weak(capability)
                            }
                        },
                    };

                    let new_cap_id = dst.[<insert_ $cap_name>](new_capability)?;

                    if destroy_old_cap {
                        // panic safety: this capability is already checked to exist
                        src.[<remove_ $cap_name>](cap_id).unwrap();
                    }

                    Ok(new_cap_id)
                }
            }
        }
    };
}

generate_cap_methods!(CapabilityMap, Process, process_map, process);
generate_cap_methods!(CapabilityMap, Memory, memory_map, memory);
generate_cap_methods!(CapabilityMap, Spawner, spawner_map, spawner);
generate_cap_methods!(CapabilityMap, Key, key_map, key);
generate_cap_methods!(CapabilityMap, Channel, channel_map, channel);
generate_cap_methods!(CapabilityMap, CapAllocator, allocator_map, allocator);
generate_cap_methods!(CapabilityMap, DropCheck, drop_check_map, drop_check);
generate_cap_methods!(CapabilityMap, DropCheckReciever, drop_check_reciever_map, drop_check_reciever);

impl CapabilityMap {
    /// Gets a userspace buffer from the given memory id and size and offset
    pub fn get_userspace_buffer(
        &self,
        memory_id: usize,
        buffer_offset: usize,
        buffer_size: usize,
        required_perms: CapFlags,
        weak_auto_destroy: bool,
    ) -> KResult<UserspaceBuffer> {
        let memory = self.get_memory_with_perms(memory_id, required_perms, weak_auto_destroy)?
            .downgrade()
            .into_inner();

        Ok(UserspaceBuffer::new(memory, buffer_offset, buffer_size))
    }
}