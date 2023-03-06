use core::sync::atomic::{AtomicUsize, Ordering};

use crate::{prelude::*, alloc::AllocRef};
use crate::container::HashMap;
use crate::process::{Process, Spawner};
use crate::alloc::CapAllocator;
use crate::sync::IMutex;
use crate::container::Arc;

use super::{CapId, Capability, CapFlags, CapObject, key::Key};

type InnerCapMap<T> = IMutex<HashMap<CapId, Capability<T>>>;

/// A map that holds all the capabilities in a process
#[derive(Debug)]
pub struct CapabilityMap {
    next_id: AtomicUsize,
    process_map: InnerCapMap<Process>,
    key_map: InnerCapMap<Key>,
    spawner_map: InnerCapMap<Spawner>,
    allocator_map: InnerCapMap<CapAllocator>,
}

impl CapabilityMap {
    pub fn new(allocator: AllocRef) -> Self {
        CapabilityMap {
            next_id: AtomicUsize::new(0),
            process_map: IMutex::new(HashMap::new(allocator.clone())),
            key_map: IMutex::new(HashMap::new(allocator.clone())),
            spawner_map: IMutex::new(HashMap::new(allocator.clone())),
            allocator_map: IMutex::new(HashMap::new(allocator)),
        }
    }
}

macro_rules! generate_cap_methods {
    ($map:ty, $cap_type:ty, $cap_map:ident, $cap_name:ident) => {
        impl $map {
            concat_idents::concat_idents!(insert_cap = insert_, $cap_name {
                pub fn insert_cap(&self, capability: Capability<$cap_type>) -> KResult<CapId> {
                    let next_id = self.next_id.fetch_add(1, Ordering::Relaxed);
                    
                    let cap_id = CapId::new(
                        $cap_type::TYPE,
                        capability.flags(),
                        capability.is_weak(),
                        next_id
                    );

                    self.$cap_map.lock().insert(cap_id, capability)?;
                    Ok(cap_id)
                }
            });

            concat_idents::concat_idents!(get_with_perms = get_, $cap_name, _with_perms {
                pub fn get_with_perms(
                    &self,
                    cap_id: usize,
                    required_perms: CapFlags,
                    weak_auto_destroy: bool
                ) -> KResult<Arc<$cap_type>> {
                    let mut map = self.$cap_map.lock();

                    let cap_id = CapId::try_from(cap_id).ok_or(SysErr::InvlId)?;
                    let cap = map.get(&cap_id).ok_or(SysErr::InvlId)?;

                    if !cap.flags().contains(required_perms) {
                        return Err(SysErr::InvlPerm)
                    }

                    Ok(match cap {
                        Capability::Strong(cap) => cap.inner().clone(),
                        Capability::Weak(cap) => {
                            let strong = cap.inner().upgrade();

                            match strong {
                                Some(cap) => cap,
                                None => {
                                    if weak_auto_destroy {
                                        map.remove(&cap_id);
                                    }

                                    return Err(SysErr::InvlWeak);
                                }
                            }
                        },
                    })
                }
            });
        }
    };
}

generate_cap_methods!(CapabilityMap, Process, process_map, process);
generate_cap_methods!(CapabilityMap, Spawner, spawner_map, spawner);
generate_cap_methods!(CapabilityMap, Key, key_map, key);
generate_cap_methods!(CapabilityMap, CapAllocator, allocator_map, allocator);