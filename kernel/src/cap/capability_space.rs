use core::sync::atomic::{AtomicUsize, Ordering};

use paste::paste;
use sys::CapType;

use crate::event::{UserspaceBuffer, EventPool};
use crate::sched::{ThreadGroup, Thread};
use crate::{prelude::*, alloc::HeapRef};
use crate::container::HashMap;
use crate::alloc::CapAllocator;
use crate::sync::IMutex;
use crate::container::Arc;
use super::address_space::AddressSpace;
use super::drop_check::{DropCheck, DropCheckReciever};
use super::{CapId, Capability, StrongCapability, CapFlags, CapObject, key::Key, memory::Memory, channel::Channel};

type InnerCapMap<T> = IMutex<HashMap<CapId, Capability<T>>>;

/// A map that holds all the capabilities in a process
#[derive(Debug)]
pub struct CapabilitySpace {
    next_id: AtomicUsize,
    thread_map: InnerCapMap<Thread>,
    thread_group_map: InnerCapMap<ThreadGroup>,
    address_space_map: InnerCapMap<AddressSpace>,
    capability_space_map: InnerCapMap<Self>,
    memory_map: InnerCapMap<Memory>,
    event_pool_map: InnerCapMap<EventPool>,
    key_map: InnerCapMap<Key>,
    channel_map: InnerCapMap<Channel>,
    allocator_map: InnerCapMap<CapAllocator>,
    drop_check_map: InnerCapMap<DropCheck>,
    drop_check_reciever_map: InnerCapMap<DropCheckReciever>,
}

impl CapabilitySpace {
    pub fn new(allocator: HeapRef) -> Self {
        CapabilitySpace {
            next_id: AtomicUsize::new(0),
            thread_map: IMutex::new(HashMap::new(allocator.clone())),
            thread_group_map: IMutex::new(HashMap::new(allocator.clone())),
            address_space_map: IMutex::new(HashMap::new(allocator.clone())),
            capability_space_map: IMutex::new(HashMap::new(allocator.clone())),
            memory_map: IMutex::new(HashMap::new(allocator.clone())),
            event_pool_map: IMutex::new(HashMap::new(allocator.clone())),
            key_map: IMutex::new(HashMap::new(allocator.clone())),
            channel_map: IMutex::new(HashMap::new(allocator.clone())),
            allocator_map: IMutex::new(HashMap::new(allocator.clone())),
            drop_check_map: IMutex::new(HashMap::new(allocator.clone())),
            drop_check_reciever_map: IMutex::new(HashMap::new(allocator)),
        }
    }

    /// Gets the CapabilitySpace of the current thread
    pub fn current() -> Arc<Self> {
        cpu_local_data().current_thread().capability_space().clone()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapCloneWeakness {
    KeepSame,
    MakeStrong,
    MakeWeak,
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
                    cap_weakness: CapCloneWeakness,
                    destroy_old_cap: bool,
                    weak_auto_destroy: bool,
                ) -> KResult<CapId> {
                    let capability = src.[<get_ $cap_name>](cap_id)?;
                    
                    let make_strong_cap = match cap_weakness {
                        CapCloneWeakness::KeepSame => !capability.is_weak(),
                        CapCloneWeakness::MakeStrong => true,
                        CapCloneWeakness::MakeWeak => false,
                    };

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
                        // ignore this error, if it occurs it means someone else has already destroyed the capability
                        let _ = src.[<remove_ $cap_name>](cap_id);
                    }

                    Ok(new_cap_id)
                }
            }
        }
    };
}

generate_cap_methods!(CapabilitySpace, Thread, thread_map, thread);
generate_cap_methods!(CapabilitySpace, ThreadGroup, thread_group_map, thread_group);
generate_cap_methods!(CapabilitySpace, AddressSpace, address_space_map, address_space);
generate_cap_methods!(CapabilitySpace, CapabilitySpace, capability_space_map, capability_space);
generate_cap_methods!(CapabilitySpace, Memory, memory_map, memory);
generate_cap_methods!(CapabilitySpace, EventPool, event_pool_map, event_pool);
generate_cap_methods!(CapabilitySpace, Key, key_map, key);
generate_cap_methods!(CapabilitySpace, Channel, channel_map, channel);
generate_cap_methods!(CapabilitySpace, CapAllocator, allocator_map, allocator);
generate_cap_methods!(CapabilitySpace, DropCheck, drop_check_map, drop_check);
generate_cap_methods!(CapabilitySpace, DropCheckReciever, drop_check_reciever_map, drop_check_reciever);

impl CapabilitySpace {
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

        let memory_cap_id = CapId::try_from(memory_id)
            .ok_or(SysErr::InvlId)?;

        Ok(UserspaceBuffer::new(memory_cap_id, memory, buffer_offset, buffer_size))
    }
}

impl CapObject for CapabilitySpace {
    const TYPE: CapType = CapType::CapabilitySpace;
}