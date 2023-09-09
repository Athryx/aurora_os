use core::cmp::min;

use sys::{CapType, MemoryResizeFlags};

use crate::alloc::{HeapRef, PaRef};
use crate::prelude::*;
use crate::sync::{IMutex, IMutexGuard};
use crate::vmem_manager::{VirtAddrSpace, PageMappingFlags};
use crate::container::{Arc, HashMap};

use super::memory::MemoryId;
use super::{CapObject, memory::{Memory, MemoryInner}};

#[derive(Debug)]
pub struct AddressSpace {
    inner: IMutex<AddressSpaceInner>,
    cr3: PhysAddr,
}

impl AddressSpace {
    pub fn new(page_allocator: PaRef, heap_allocator: HeapRef) -> KResult<Self> {
        let addr_space = VirtAddrSpace::new(page_allocator, heap_allocator.clone())?;

        Ok(AddressSpace {
            cr3: addr_space.cr3_addr(),
            inner: IMutex::new(AddressSpaceInner {
                addr_space,
                mappings: HashMap::new(heap_allocator.clone()),
                memory_id_map_addr: HashMap::new(heap_allocator),
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

    /// Maps `memory` at `addr`
    /// 
    /// if `max_size_pages` is `Some(_)`, the mapped memory will take up no more than `max_size_pages` pages in the virtual address space
    /// 
    /// `flags` specifies the read, write, and execute permissions, but the memory is always mapped as user
    /// 
    /// # Returns
    /// 
    /// the size of the mapping
    /// 
    /// # Locking
    /// 
    /// acquires `inner` lock on address space
    /// acquires the `inner` lock on the memory capability
    pub fn map_memory(
        &self,
        memory: Arc<Memory>,
        addr: VirtAddr,
        max_size: Option<Size>,
        flags: PageMappingFlags,
    ) -> KResult<Size> {
        let mut addr_space_inner = self.inner.lock();
        let mut memory_inner = memory.inner_write();

        let mapping_size = min(
            max_size.unwrap_or(memory_inner.size()),
            memory_inner.size(),
        );
        if mapping_size.is_zero() {
            return Err(SysErr::InvlArgs);
        }

        addr_space_inner.insert_mapping(addr, memory.id(), AddrSpaceMapping {
            memory: memory.clone(),
            addr,
            size: mapping_size,
            flags,
        })?;

        let map_result = addr_space_inner.addr_space.map_many(
            memory_inner.iter_mapped_regions(
                addr,
                Size::zero(),
                mapping_size,
            ),
            flags,
        );

        if let Err(error) = map_result {
            // if mapping failed, remove entry from mapped_memory_capabilities
            // panic safety: mapping was just inserted
            addr_space_inner.remove_mapping_from_id(memory.id()).unwrap();

            Err(error)
        } else {
            memory_inner.map_ref_count += 1;

            Ok(mapping_size)
        }
    }

    /// Unmaps the memory that was mapped at `addr`
    /// 
    /// # Locking
    /// 
    /// acquires `inner` lock on address space
    /// acquires the `inner` lock on the memory capability
    pub fn unmap_memory(&self, addr: VirtAddr) -> KResult<()> {
        let mut addr_space_inner = self.inner.lock();

        let Some(mapping) = addr_space_inner.remove_mapping_from_address(addr) else {
            // no memory was mapped at the given address
            return Err(SysErr::InvlVirtAddr);
        };

        let mut memory_inner = mapping.memory.inner_write();

        for (virt_range, _) in memory_inner.iter_mapped_regions(
            mapping.addr,
            Size::zero(),
            mapping.size,
        ) {
            // this should not fail because we ensure that memory was already mapped
            addr_space_inner.addr_space.unmap_memory(virt_range)
                .expect("failed to unmap memory that should have been mapped");
        }

        memory_inner.map_ref_count -= 1;

        Ok(())
    }

    /// Updates the mapping for the given memory capability
    /// 
    /// # Returns
    /// 
    /// Returns the size of the new mapping in pages
    /// 
    /// # Locking
    /// 
    /// acquires `inner` lock on address space
    /// acquires the `inner` lock on the memory capability
    pub fn update_memory_mapping(&self, addr: VirtAddr, max_size: Option<Size>) -> KResult<Size> {
        let mut addr_space_inner = self.inner.lock();

        let mapping = addr_space_inner.mappings.get(&addr)
            .ok_or(SysErr::InvlVirtAddr)?
            .clone();

        let mut memory_inner = mapping.memory.inner_write();

        addr_space_inner.update_memory_mapping_inner(&mapping, &mut memory_inner, max_size)        
    }

    /// Resizes the specified memory capability specified by `memory` to be the size of `new_size_pages`
    /// 
    /// If `resize_in_place` is true, the memory can be resized even if it is currently mapped
    /// 
    /// # Returns
    /// 
    /// returns the new size of the memory in pages
    /// 
    /// # Locking
    /// 
    /// acquires `inner` lock on address space
    /// acquires the `inner` lock on the memory capability
    pub fn resize_memory(
        &self,
        memory: Arc<Memory>,
        new_size: Size,
        flags: MemoryResizeFlags,
    ) -> KResult<Size> {
        let mut addr_space_inner = self.inner.lock();
        let mut memory_inner = memory.inner_write();

        let old_size = memory_inner.size();
        if old_size == new_size {
            return Ok(old_size);
        }

        if memory_inner.map_ref_count == 0 {
            // Safety: map ref count is checked to be 0, os this capability is not mapped in memory
            unsafe {
                memory_inner.resize_out_of_place(new_size.pages_rounded())?;
            }

            Ok(memory_inner.size())
        } else if flags.contains(MemoryResizeFlags::IN_PLACE) && memory_inner.map_ref_count == 1 {
            let mapping = addr_space_inner.get_mapping_from_id(memory.id())
                .ok_or(SysErr::InvlOp)?
                .clone();

            if new_size > old_size {
                unsafe {
                    memory_inner.resize_in_place(new_size.pages_rounded())?;
                }

                let memory_size = memory_inner.size();
                if flags.contains(MemoryResizeFlags::GROW_MAPPING) {
                    addr_space_inner.update_memory_mapping_inner(
                        &mapping,
                        &mut memory_inner,
                        Some(memory_size)
                    )?;
                }

                Ok(memory_size)
            } else if new_size < old_size {
                // shrink memory
                if mapping.size > new_size {
                    addr_space_inner.update_memory_mapping_inner(
                        &mapping,
                        &mut memory_inner,
                        Some(new_size)
                    )?;
                }
                
                // panic safety: shrinking the allocated memory should never fail
                unsafe {
                    memory_inner.resize_in_place(new_size.pages_rounded()).unwrap();
                }

                Ok(memory_inner.size())
            } else {
                Ok(memory_inner.size())
            }
        } else {
            Err(SysErr::InvlOp)
        }
    }

    /// Used to get dirrect access to inner address space
    /// 
    /// This shouldn't be used usually, only event pool uses it
    pub fn inner(&self) -> IMutexGuard<AddressSpaceInner> {
        self.inner.lock()
    }
}

impl CapObject for AddressSpace {
    const TYPE: CapType = CapType::AddressSpace;
}

/// Represents where in the address space a capability was mapped
#[derive(Debug, Clone)]
struct AddrSpaceMapping {
    memory: Arc<Memory>,
    addr: VirtAddr,
    size: Size,
    flags: PageMappingFlags,
}

#[derive(Debug)]
pub struct AddressSpaceInner {
    pub addr_space: VirtAddrSpace,
    /// A map between thr address of a mapping and the details of what is mappoed
    mappings: HashMap<VirtAddr, AddrSpaceMapping>,
    /// Which address the memory with the given id is mapped at
    memory_id_map_addr: HashMap<MemoryId, VirtAddr>,
}

impl AddressSpaceInner {
    fn insert_mapping(
        &mut self,
        address: VirtAddr,
        memory_id: MemoryId,
        mapping: AddrSpaceMapping,
    ) -> KResult<()> {
        self.memory_id_map_addr.insert(memory_id, address)?;

        if let Err(error) = self.mappings.insert(address, mapping) {
            // panic safety: this was just inserted
            self.memory_id_map_addr.remove(&memory_id).unwrap();
            Err(error)
        } else {
            Ok(())
        }
    }

    fn remove_mapping_from_address(&mut self, address: VirtAddr) -> Option<AddrSpaceMapping> {
        let mapping = self.mappings.remove(&address)?;
        self.memory_id_map_addr.remove(&mapping.memory.id())
            .expect("mapping id was not present in memory");

        Some(mapping)
    }

    fn remove_mapping_from_id(&mut self, memory_id: MemoryId) -> Option<AddrSpaceMapping> {
        let mapping_addr = self.memory_id_map_addr.remove(&memory_id)?;
        self.mappings.remove(&mapping_addr)
    }

    fn get_mapping_from_id(&self, memory_id: MemoryId) -> Option<&AddrSpaceMapping> {
        let mapping_addr = self.memory_id_map_addr.get(&memory_id)?;
        self.mappings.get(&mapping_addr)
    }

    fn update_memory_mapping_inner(
        &mut self,
        mapping: &AddrSpaceMapping,
        memory_inner: &mut MemoryInner,
        max_size: Option<Size>,
    ) -> KResult<Size> {
        let old_size = mapping.size;
        let new_size = max_size.unwrap_or(mapping.size);
        if new_size.is_zero() {
            return Err(SysErr::InvlArgs);
        }
    
        if new_size > old_size {
            let new_base_addr = mapping.addr + old_size.bytes();

            let mapping_iter = memory_inner.iter_mapped_regions(
                new_base_addr,
                Size::zero(),
                new_size - old_size,
            );

            // must map new regions first before resizing old mapping
            let flags = mapping.flags | PageMappingFlags::USER;
            self.addr_space.map_many(
                mapping_iter.clone().without_unaligned_start(),
                flags,
            )?;

            let result = self.addr_space.resize_mapping(mapping_iter.get_entire_first_maping_range());

            if let Err(error) = result {
                for (virt_range, _) in mapping_iter {
                    // panic safety: this memory was just mapped so this is guarenteed to not fail
                    self.addr_space.unmap_memory(virt_range).unwrap();
                }

                Err(error)
            } else {
                Ok(new_size)
            }
        } else if new_size < old_size {
            let unmap_base_addr = mapping.addr + new_size.bytes();

            let mapping_iter = memory_inner.iter_mapped_regions(
                unmap_base_addr,
                Size::zero(),
                old_size - new_size,
            );

            // first resize the overlapping part
            self.addr_space.resize_mapping(mapping_iter.get_first_mapping_exluded_range())?;

            // now unmap everything else
            for (virt_range, _) in mapping_iter.without_unaligned_start() {
                // panic safety: this memory should be mapped
                self.addr_space.unmap_memory(virt_range).unwrap();
            }

            Ok(new_size)
        } else {
            Ok(old_size)
        }
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