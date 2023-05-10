use core::sync::atomic::{AtomicUsize, Ordering};

use concat_idents::concat_idents;

use crate::{prelude::*, alloc::HeapRef};
use crate::container::HashMap;
use crate::process::{Process, Spawner};
use crate::alloc::CapAllocator;
use crate::sync::IMutex;

use super::{CapId, Capability, StrongCapability, CapFlags, CapObject, key::Key, memory::Memory};

type InnerCapMap<T> = IMutex<HashMap<CapId, Capability<T>>>;

/// A map that holds all the capabilities in a process
#[derive(Debug)]
pub struct CapabilityMap {
    next_id: AtomicUsize,
    process_map: InnerCapMap<Process>,
    memory_map: InnerCapMap<Memory>,
    key_map: InnerCapMap<Key>,
    spawner_map: InnerCapMap<Spawner>,
    allocator_map: InnerCapMap<CapAllocator>,
}

impl CapabilityMap {
    pub fn new(allocator: HeapRef) -> Self {
        CapabilityMap {
            next_id: AtomicUsize::new(0),
            process_map: IMutex::new(HashMap::new(allocator.clone())),
            memory_map: IMutex::new(HashMap::new(allocator.clone())),
            key_map: IMutex::new(HashMap::new(allocator.clone())),
            spawner_map: IMutex::new(HashMap::new(allocator.clone())),
            allocator_map: IMutex::new(HashMap::new(allocator)),
        }
    }
}

macro_rules! generate_cap_methods {
    ($map:ty, $cap_type:ty, $cap_map:ident, $cap_name:ident) => {
        impl $map {
            concat_idents!(insert_cap = insert_, $cap_name {
                pub fn insert_cap(&self, mut capability: Capability<$cap_type>) -> KResult<CapId> {
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
            });

            concat_idents!(get_strong_with_perms = get_strong_, $cap_name, _with_perms {
                pub fn get_strong_with_perms(
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
            });

            concat_idents!(get_with_perms = get_, $cap_name, _with_perms {
                pub fn get_with_perms(
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
            });

            concat_idents!(get_strong_cap = get_strong_, $cap_name {
                pub fn get_strong_cap(
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
            });

            concat_idents!(get_cap = get_, $cap_name {
                pub fn get_cap(&self, cap_id: CapId) -> KResult<Capability<$cap_type>> {
                    let map = self.$cap_map.lock();

                    Ok(map.get(&cap_id).ok_or(SysErr::InvlId)?.clone())
                }
            });
        }
    };
}

generate_cap_methods!(CapabilityMap, Process, process_map, process);
generate_cap_methods!(CapabilityMap, Memory, memory_map, memory);
generate_cap_methods!(CapabilityMap, Spawner, spawner_map, spawner);
generate_cap_methods!(CapabilityMap, Key, key_map, key);
generate_cap_methods!(CapabilityMap, CapAllocator, allocator_map, allocator);