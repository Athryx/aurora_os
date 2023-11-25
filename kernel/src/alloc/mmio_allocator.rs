use sys::CapType;

use crate::cap::CapObject;
use crate::cap::address_space::{AddressSpace, PhysMemMapping, AddrSpaceMapping, AddressSpaceInner, MappingId};
use crate::prelude::*;
use crate::vmem_manager::{MapAction, PageMappingOptions};

use super::HeapRef;

/// Let userspace programs allocate memory corresponding to a given physical address
/// 
/// This is used for drivers to interface with hardware
#[derive(Debug)]
pub struct MmioAllocator {
    /// Reserved regions are memory regions that cannot be allocated as PhysMem
    /// 
    /// This includes memory storing kernel code and data and all memory used by regular page allocator
    reserved_regions: Vec<APhysRange>,
}

impl MmioAllocator {
    pub fn new(allocator: HeapRef) -> Self {
        MmioAllocator {
            reserved_regions: Vec::new(allocator),
        }
    }

    fn get_reserved_region_insert_index(&self, region: APhysRange) -> Option<usize> {
        match self.reserved_regions.binary_search_by_key(&region.addr(), |region| region.addr()) {
            // this address is already occupied
            Ok(_) => None,
            Err(index) => {
                if (index == 0 || self.reserved_regions[index - 1].end_addr() <= region.addr())
                    && (index == self.reserved_regions.len() || region.end_addr() <= self.reserved_regions[index].addr()) {
                    Some(index)
                } else {
                    // this region overlaps an already reserved region
                    None
                }
            },
        }
    }

    fn overlaps_reserved_region(&self, region: APhysRange) -> bool {
        self.get_reserved_region_insert_index(region).is_none()
    }

    /// Marks a new region as reserved
    pub(super) fn add_reserved_region(&mut self, reserved_region: APhysRange) -> KResult<()> {
        if let Some(index) = self.get_reserved_region_insert_index(reserved_region) {
            self.reserved_regions.insert(index, reserved_region)
        } else {
            Err(SysErr::InvlMemZone)
        }
    }

    /// Tries to allocate the memory region and returns a PhysMem capability for that region
    pub fn alloc(&self, region: APhysRange) -> KResult<PhysMem> {
        if self.overlaps_reserved_region(region) {
            Err(SysErr::InvlMemZone)
        } else {
            Ok(PhysMem { region })
        }
    }
}

impl CapObject for MmioAllocator {
    const TYPE: CapType = CapType::MmioAllocator;
}

struct MmioAllocatorInner {
    allocated_zones: Vec<APhysRange>,
}

#[derive(Debug, Clone, Copy)]
pub struct PhysMem {
    region: APhysRange,
}

impl PhysMem {
    pub fn map(&self, address_space: &AddressSpace, address: VirtAddr, options: PageMappingOptions) -> KResult<Size> {
        let mut addr_space_inner = address_space.inner();

        let mapping = PhysMemMapping {
            phys_mem: *self,
            map_range: AVirtRange::new(address, self.region.size()),
            options,
            map_id: MappingId::new(),
        };

        addr_space_inner.mappings.insert_mapping(AddrSpaceMapping::PhysMem(mapping))?;

        let map_result = unsafe {
            addr_space_inner.addr_space.map_many(self.iter_mapping(address, options))
        };

        if let Err(error) = map_result {
            // panic safety: this mapping was just inserted
            addr_space_inner.mappings.remove_mapping_from_address(address).unwrap();
            Err(error)
        } else {
            Ok(Size::from_bytes(self.region.size()))
        }
    }

    pub fn unmap(&self, address_space: &mut AddressSpaceInner, address: VirtAddr) -> KResult<()> {
        let mapping = address_space.mappings.remove_mapping_from_address(address)
            .ok_or(SysErr::InvlVirtAddr)?;

        let AddrSpaceMapping::PhysMem(mapping) = mapping else {
            panic!("tried to unmap regular memory with physmem unmap");
        };

        for map_action in self.iter_mapping(address, mapping.options) {
            unsafe {
                address_space.addr_space.unmap_page(map_action.virt_addr).expect("failed to unmap physmem page");
            }
        }

        Ok(())
    }

    pub fn size(&self) -> Size {
        Size::from_bytes(self.region.size())
    }

    fn iter_mapping(&self, address: VirtAddr, options: PageMappingOptions) -> impl Iterator<Item = MapAction> + Clone {
        let map_page_count = self.region.page_size();
        let phys_addr = self.region.addr();

        (0..map_page_count).map(move |i| MapAction {
            virt_addr: address + PAGE_SIZE * i,
            phys_addr: phys_addr + PAGE_SIZE * i,
            options,
        })
    }
}

impl CapObject for PhysMem {
    const TYPE: CapType = CapType::PhysMem;
}