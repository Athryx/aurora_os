use paste::paste;
use sys::CapType;

use crate::event::{UserspaceBuffer, EventPool};
use crate::int::userspace_interrupt::{IntAllocator, Interrupt};
use crate::sched::{ThreadGroup, Thread};
use crate::{prelude::*, alloc::HeapRef};
use crate::container::HashMap;
use crate::alloc::{CapAllocator, MmioAllocator, PhysMem};
use crate::sync::IMutex;
use crate::container::Arc;
use super::address_space::AddressSpace;
use super::drop_check::{DropCheck, DropCheckReciever};
use super::{CapId, Capability, StrongCapability, CapFlags, CapObject, key::Key, memory::Memory, channel::{Channel, Reply}};

#[derive(Debug)]
struct CapabilityEntry<T: CapObject> {
    visible: bool,
    capability: Capability<T>,
}

/// Represents a map for one type of capability
#[derive(Debug)]
struct InnerCapMap<T: CapObject> {
    next_id: usize,
    map: HashMap<CapId, CapabilityEntry<T>>,
}

impl<T: CapObject> InnerCapMap<T> {
    fn new(allocator: HeapRef) -> Self {
        InnerCapMap {
            next_id: 0,
            map: HashMap::new(allocator),
        }
    }

    fn next_id(&mut self) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    fn insert_capability(&mut self, mut capability: Capability<T>, visible: bool) -> KResult<CapId> {
        let cap_id = CapId::new(
            T::TYPE,
            capability.flags(),
            capability.is_weak(),
            self.next_id(),
        );

        capability.set_id(cap_id);

        self.map.insert(cap_id, CapabilityEntry {
            capability,
            visible,
        })?;

        Ok(cap_id)
    }

    /// Atomicly inserts capabilties from the iterator with the given flags, and returns the base id
    fn insert_multiple_capabilities(&mut self, capabilities: impl ExactSizeIterator<Item = KResult<StrongCapability<T>>>, flags: CapFlags) -> KResult<CapId> {
        let base_id = self.next_id;
        let capability_count = capabilities.len();
        self.next_id += capability_count;

        let mut current_index = 0;

        let result: KResult<CapId> = try {
            for (i, capability) in capabilities.enumerate() {
                current_index = i;

                let mut capability = Capability::Strong(capability?);

                let cap_id = CapId::new(
                    T::TYPE,
                    flags,
                    false,
                    base_id + i,
                );
    
                capability.set_id(cap_id);
    
                // insert as invisible first
                self.map.insert(cap_id, CapabilityEntry {
                    capability,
                    visible: false,
                })?;
            }

            // insertion succeeded, mark capabilities as visible
            for i in 0..capability_count {
                let cap_id = CapId::new(
                    T::TYPE,
                    flags,
                    false,
                    base_id + i,
                );

                self.map.get_mut(&cap_id).expect("capability which should be inserted not found")
                    .visible = true;
            }

            CapId::new(
                T::TYPE,
                flags,
                false,
                base_id
            )
        };

        // insertion succeeded, return
        if result.is_ok() {
            return result;
        }

        // inserion of all capabilties failed, remove capabilities that were inserted
        // failure occured on current index
        for i in 0..current_index {
            let cap_id = CapId::new(
                T::TYPE,
                flags,
                false,
                base_id + i,
            );

            // ignore error, it means somone else handled the error
            let _ = self.map.remove(&cap_id);
        }

        // rollback id counter to before insertion
        self.next_id = base_id;

        result
    }

    fn set_capability_visibility(&mut self, cap_id: CapId, visible: bool) -> KResult<()> {
        let cap_entry = self.map.get_mut(&cap_id).ok_or(SysErr::InvlId)?;
        cap_entry.visible = visible;
        Ok(())
    }

    fn get_capability(&self, cap_id: CapId) -> KResult<Capability<T>> {
        let cap_entry = self.map.get(&cap_id).ok_or(SysErr::InvlId)?;

        if cap_entry.visible {
            Ok(cap_entry.capability.clone())
        } else {
            Err(SysErr::InvlId)
        }
    }

    fn remove_capability(&mut self, cap_id: CapId) -> KResult<Capability<T>> {
        Ok(
            self.map.remove(&cap_id).ok_or(SysErr::InvlId)?.capability
        )
    }
}

/// A map that holds all the capabilities in a process
#[derive(Debug)]
pub struct CapabilitySpace {
    thread_map: IMutex<InnerCapMap<Thread>>,
    thread_group_map: IMutex<InnerCapMap<ThreadGroup>>,
    address_space_map: IMutex<InnerCapMap<AddressSpace>>,
    capability_space_map: IMutex<InnerCapMap<Self>>,
    memory_map: IMutex<InnerCapMap<Memory>>,
    event_pool_map: IMutex<InnerCapMap<EventPool>>,
    key_map: IMutex<InnerCapMap<Key>>,
    channel_map: IMutex<InnerCapMap<Channel>>,
    reply_map: IMutex<InnerCapMap<Reply>>,
    allocator_map: IMutex<InnerCapMap<CapAllocator>>,
    drop_check_map: IMutex<InnerCapMap<DropCheck>>,
    drop_check_reciever_map: IMutex<InnerCapMap<DropCheckReciever>>,
    mmio_allocator_map: IMutex<InnerCapMap<MmioAllocator>>,
    phys_mem_map: IMutex<InnerCapMap<PhysMem>>,
    int_allocator_map: IMutex<InnerCapMap<IntAllocator>>,
    interrupt_map: IMutex<InnerCapMap<Interrupt>>,
}

impl CapabilitySpace {
    pub fn new(allocator: HeapRef) -> Self {
        CapabilitySpace {
            thread_map: IMutex::new(InnerCapMap::new(allocator.clone())),
            thread_group_map: IMutex::new(InnerCapMap::new(allocator.clone())),
            address_space_map: IMutex::new(InnerCapMap::new(allocator.clone())),
            capability_space_map: IMutex::new(InnerCapMap::new(allocator.clone())),
            memory_map: IMutex::new(InnerCapMap::new(allocator.clone())),
            event_pool_map: IMutex::new(InnerCapMap::new(allocator.clone())),
            key_map: IMutex::new(InnerCapMap::new(allocator.clone())),
            channel_map: IMutex::new(InnerCapMap::new(allocator.clone())),
            reply_map: IMutex::new(InnerCapMap::new(allocator.clone())),
            allocator_map: IMutex::new(InnerCapMap::new(allocator.clone())),
            drop_check_map: IMutex::new(InnerCapMap::new(allocator.clone())),
            drop_check_reciever_map: IMutex::new(InnerCapMap::new(allocator.clone())),
            mmio_allocator_map: IMutex::new(InnerCapMap::new(allocator.clone())),
            phys_mem_map: IMutex::new(InnerCapMap::new(allocator.clone())),
            int_allocator_map: IMutex::new(InnerCapMap::new(allocator.clone())),
            interrupt_map: IMutex::new(InnerCapMap::new(allocator)),
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
                pub fn [<insert_ $cap_name _inner>](&self, capability: Capability<$cap_type>, visible: bool) -> KResult<CapId> {
                    self.$cap_map.lock().insert_capability(capability, visible)
                }

                pub fn [<insert_ $cap_name>](&self, capability: Capability<$cap_type>) -> KResult<CapId> {
                    self.[<insert_ $cap_name _inner>](capability, true)
                }

                pub fn [<insert_ $cap_name _invisible>](&self, capability: Capability<$cap_type>) -> KResult<CapId> {
                    self.[<insert_ $cap_name _inner>](capability, false)
                }

                pub fn [<insert_ $cap_name _multiple>](&self, capabilities: impl ExactSizeIterator<Item = KResult<StrongCapability<$cap_type>>>, flags: CapFlags) -> KResult<CapId> {
                    self.$cap_map.lock().insert_multiple_capabilities(capabilities, flags)
                }

                pub fn [<make_ $cap_name _visible>](&self, cap_id: CapId) -> KResult<()> {
                    self.$cap_map.lock().set_capability_visibility(cap_id, true)
                }

                pub fn [<remove_ $cap_name>](&self, cap_id: CapId) -> KResult<Capability<$cap_type>> {
                    self.$cap_map.lock().remove_capability(cap_id)
                }

                /// Gets a strong reference to the given capability, and checks that it has the required permissions
                pub fn [<get_ $cap_name _with_perms>](
                    &self,
                    cap_id: usize,
                    required_perms: CapFlags,
                    weak_auto_destroy: bool,
                ) -> KResult<StrongCapability<$cap_type>> {
                    let cap_id = CapId::try_from(cap_id).ok_or(SysErr::InvlId)?;
                    let capability = self.$cap_map.lock().get_capability(cap_id)?;

                    if !capability.flags().contains(required_perms) {
                        return Err(SysErr::InvlPerm);
                    }

                    match &capability {
                        Capability::Strong(cap) => Ok(cap.clone()),
                        Capability::Weak(cap) => {
                            let strong = cap.upgrade();

                            match strong {
                                Some(cap) => Ok(cap),
                                None => {
                                    if weak_auto_destroy {
                                        // ignore error if capability was already removed by someone else
                                        let _ = self.$cap_map.lock().remove_capability(cap_id);
                                    }

                                    Err(SysErr::InvlWeak)
                                }
                            }
                        },
                    }
                }

                pub fn [<get_ $cap_name>](&self, cap_id: CapId) -> KResult<Capability<$cap_type>> {
                    self.$cap_map.lock().get_capability(cap_id)
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
generate_cap_methods!(CapabilitySpace, Reply, reply_map, reply);
generate_cap_methods!(CapabilitySpace, CapAllocator, allocator_map, allocator);
generate_cap_methods!(CapabilitySpace, DropCheck, drop_check_map, drop_check);
generate_cap_methods!(CapabilitySpace, DropCheckReciever, drop_check_reciever_map, drop_check_reciever);
generate_cap_methods!(CapabilitySpace, MmioAllocator, mmio_allocator_map, mmio_allocator);
generate_cap_methods!(CapabilitySpace, PhysMem, phys_mem_map, phys_mem);
generate_cap_methods!(CapabilitySpace, IntAllocator, int_allocator_map, int_allocator);
generate_cap_methods!(CapabilitySpace, Interrupt, interrupt_map, interrupt);

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
            .into_inner();

        Ok(UserspaceBuffer::new(memory, buffer_offset, buffer_size))
    }

    pub fn cap_clone(
        dst_cspace: &CapabilitySpace,
        src_cspace: &CapabilitySpace,
        cap_id: CapId,
        new_cap_perms: CapFlags,
        cap_weakness: CapCloneWeakness,
        destroy_src_cap: bool,
        weak_auto_destroy: bool,
    ) -> KResult<CapId> {
        macro_rules! call_cap_clone {
            ($cspace_clone:ident) => {
                CapabilitySpace::$cspace_clone(
                    &dst_cspace,
                    &src_cspace,
                    cap_id,
                    new_cap_perms,
                    cap_weakness,
                    destroy_src_cap,
                    weak_auto_destroy,
                )
            };
        }
    
        match cap_id.cap_type() {
            CapType::Thread => call_cap_clone!(clone_thread),
            CapType::ThreadGroup => call_cap_clone!(clone_thread_group),
            CapType::AddressSpace => call_cap_clone!(clone_address_space),
            CapType::CapabilitySpace => call_cap_clone!(clone_capability_space),
            CapType::Memory => call_cap_clone!(clone_memory),
            //CapType::Lock => call_cap_clone!(clone_),
            CapType::EventPool => call_cap_clone!(clone_event_pool),
            CapType::Channel => call_cap_clone!(clone_channel),
            CapType::Reply => call_cap_clone!(clone_reply),
            //CapType::MessageCapacity => call_cap_clone!(clone_),
            CapType::Key => call_cap_clone!(clone_key),
            CapType::Allocator => call_cap_clone!(clone_allocator),
            CapType::DropCheck => call_cap_clone!(clone_drop_check),
            CapType::DropCheckReciever => call_cap_clone!(clone_drop_check_reciever),
            //CapType::RootOom => call_cap_clone!(clone_),
            CapType::MmioAllocator => call_cap_clone!(clone_mmio_allocator),
            CapType::PhysMem => call_cap_clone!(clone_phys_mem),
            CapType::IntAllocator => call_cap_clone!(clone_int_allocator),
            CapType::Interrupt => call_cap_clone!(clone_interrupt),
            _ => todo!(),
        }
    }
}

impl CapObject for CapabilitySpace {
    const TYPE: CapType = CapType::CapabilitySpace;
}