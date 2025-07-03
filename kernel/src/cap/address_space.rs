use core::sync::atomic::{AtomicUsize, Ordering};

use sys::CapType;

use crate::mem::{HeapRef, PaRef, mmio_allocator::PhysMem};
use crate::consts;
use crate::event::EventPool;
use crate::prelude::*;
use crate::sync::{IMutex, IMutexGuard};
use crate::mem::vmem_manager::{VirtAddrSpace, PageMappingOptions};
use crate::container::{Arc, HashMap};

use super::memory::MemoryMappingLocation;
use super::{CapObject, memory::Memory};

crate::make_id_type!(MappingId);

static NEXT_MAPPING_ID: AtomicUsize = AtomicUsize::new(0);

impl MappingId {
    pub fn new() -> Self {
        MappingId::from(NEXT_MAPPING_ID.fetch_add(1, Ordering::Relaxed))
    }
}

#[derive(Debug)]
pub struct AddressSpace {
    inner: IMutex<AddressSpaceInner>,
    cr3: PhysAddr,
}

impl AddressSpace {
    pub fn new(page_allocator: PaRef, heap_allocator: HeapRef) -> KResult<Self> {
        let addr_space = VirtAddrSpace::new(page_allocator)?;

        Ok(AddressSpace {
            cr3: addr_space.cr3_addr(),
            inner: IMutex::new(AddressSpaceInner {
                addr_space,
                mappings: AddrSpaceMappings {
                    mappings: Vec::new(heap_allocator.clone()),
                    map_id_addrs: HashMap::new(heap_allocator),
                },
            }),
        })
    }

    /// Gets the address space of the current thread
    pub fn current() -> Arc<Self> {
        cpu_local_data().current_thread().address_space().clone()
    }

    pub fn get_cr3(&self) -> PhysAddr {
        self.cr3
    }

    /// Used to get dirrect access to inner address space
    pub fn inner(&self) -> IMutexGuard<AddressSpaceInner> {
        self.inner.lock()
    }

    pub fn unmap(&self, address: VirtAddr) -> KResult<()> {
        let mut inner = self.inner();

        let mapping = inner.mappings.get_mapping_from_address(address)
            .ok_or(SysErr::InvlVirtAddr)?;

        match mapping {
            AddrSpaceMapping::Memory(mapping) => {
                let memory = mapping.memory.clone();
                drop(inner);
                memory.unmap_memory(self, address)
            },
            AddrSpaceMapping::EventPool(mapping) => {
                let event_pool = mapping.event_pool.clone();
                drop(inner);
                event_pool.unmap()
            },
            AddrSpaceMapping::PhysMem(mapping) => {
                let phys_mem = mapping.phys_mem;
                phys_mem.unmap(&mut inner, address)
            },
        }
    }

    pub fn memory_at_addr(&self, address: VirtAddr) -> KResult<Arc<Memory>> {
        let inner = self.inner();

        let mapping = inner.mappings.get_mapping_from_address(address)
            .ok_or(SysErr::InvlVirtAddr)?;

        let AddrSpaceMapping::Memory(mapping) = mapping else {
            return Err(SysErr::InvlOp);
        };

        Ok(mapping.memory.clone())
    }
}

impl CapObject for AddressSpace {
    const TYPE: CapType = CapType::AddressSpace;
}

/// Stores details about memory mapped in the address space
#[derive(Debug, Clone)]
pub struct MemoryMapping {
    pub memory: Arc<Memory>,
    pub location: MemoryMappingLocation,
    pub mapping_id: MappingId,
}

/// Stores details about an event pool mapped in the address space
#[derive(Debug, Clone)]
pub struct EventPoolMapping {
    pub event_pool: Arc<EventPool>,
    pub map_range: AVirtRange,
}

/// Stores details about phys mem mapped in the address space
#[derive(Debug, Clone)]
pub struct PhysMemMapping {
    pub phys_mem: PhysMem,
    pub map_range: AVirtRange,
    pub options: PageMappingOptions,
    pub map_id: MappingId,
}

/// Represents where in the address space a capability was mapped
#[derive(Debug, Clone)]
pub enum AddrSpaceMapping {
    Memory(MemoryMapping),
    EventPool(EventPoolMapping),
    PhysMem(PhysMemMapping),
}

impl AddrSpaceMapping {
    // FIXME: get rid of this, it only works for regular memory
    pub fn map_id(&self) -> MappingId {
        match self {
            Self::Memory(memory) => memory.mapping_id,
            Self::EventPool(event_pool) => event_pool.event_pool.id(),
            Self::PhysMem(phys_mem) => phys_mem.map_id,
        }
    }

    pub fn map_range(&self) -> AVirtRange {
        match self {
            Self::Memory(memory) => memory.location.map_range(),
            Self::EventPool(event_pool) => event_pool.map_range,
            Self::PhysMem(phys_mem) => phys_mem.map_range,
        }
    }

    pub fn size(&self) -> Size {
        Size::from_bytes(self.map_range().size())
    }
}

#[derive(Debug)]
pub struct AddressSpaceInner {
    pub addr_space: VirtAddrSpace,
    pub mappings: AddrSpaceMappings,
}

#[derive(Debug)]
pub struct AddrSpaceMappings {
    /// A sorted list of all the mappings in this address space
    mappings: Vec<AddrSpaceMapping>,
    /// Which address the memory with the given id is mapped at
    map_id_addrs: HashMap<MappingId, VirtAddr>,
}

impl AddrSpaceMappings {
    /// Returns Some(index) if the given virt range in the virtual address space is not occupied
    /// 
    /// The index is the place where the virt_range can be inserted to maintain ordering in the list
    fn get_mapping_insert_index(&self, range: AVirtRange) -> Option<usize> {
        // can't map anything beyond the kernel region
        if range.end_usize() > *consts::KERNEL_VMA {
            return None;
        }

        match self.mappings.binary_search_by_key(&range.addr(), |mapping| mapping.map_range().addr()) {
            // If we find the address it is occupied
            Ok(_) => None,
            Err(index) => {
                if (index == 0 || self.mappings[index - 1].map_range().end_addr() <= range.addr())
                    && (index == self.mappings.len() || range.end_addr() <= self.mappings[index].map_range().addr()) {
                    Some(index)
                } else {
                    None
                }
            },
        }
    }

    /// Gets the index of the mapping starting at `address`, returns None if such a mapping does not exist
    fn get_mapping_index(&self, address: VirtAddr) -> Option<usize> {
        self.mappings
            .binary_search_by_key(&address, |mapping| mapping.map_range().addr())
            .ok()
    }

    pub fn insert_mapping(
        &mut self,
        mapping: AddrSpaceMapping,
    ) -> KResult<()> {
        let insert_index = self.get_mapping_insert_index(mapping.map_range())
            .ok_or(SysErr::InvlMemZone)?;

        let map_id = mapping.map_id();
        self.map_id_addrs.insert(map_id, mapping.map_range().addr())?;

        if let Err(error) = self.mappings.insert(insert_index, mapping) {
            // panic safety: this was just inserted
            self.map_id_addrs.remove(&map_id).unwrap();
            Err(error)
        } else {
            Ok(())
        }
    }

    pub fn remove_mapping_from_address(&mut self, address: VirtAddr) -> Option<AddrSpaceMapping> {
        let mapping = self.mappings.remove(
            self.get_mapping_index(address)?,
        );

        self.map_id_addrs.remove(&mapping.map_id())
            .expect("mapping id was not present in memory");

        Some(mapping)
    }

    pub fn remove_mapping_from_id(&mut self, memory_id: MappingId) -> Option<AddrSpaceMapping> {
        let mapping_addr = self.map_id_addrs.remove(&memory_id)?;
        
        Some(self.mappings.remove(
            self.get_mapping_index(mapping_addr)?
        ))
    }

    pub fn get_mapping_from_address(&self, address: VirtAddr) -> Option<&AddrSpaceMapping> {
        self.mappings.get(
            self.get_mapping_index(address)?
        )
    }

    pub fn get_mapping_from_address_mut(&mut self, address: VirtAddr) -> Option<&mut AddrSpaceMapping> {
        self.mappings.get_mut(
            self.get_mapping_index(address)?
        )
    }

    fn get_mapping_from_id(&self, memory_id: MappingId) -> Option<&AddrSpaceMapping> {
        let mapping_addr = self.map_id_addrs.get(&memory_id)?;

        self.mappings.get(
            self.get_mapping_index(*mapping_addr)?
        )
    }
}

impl Drop for AddressSpaceInner {
    fn drop(&mut self) {
        // safety: address space will not be loaded, because threads always keep
        // strong reference to address space, so if this is being dropped,
        // it means all threads with this address space have been dropped
        unsafe {
            self.addr_space.dealloc_addr_space()
        }
    }
}