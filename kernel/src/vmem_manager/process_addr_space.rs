use alloc::vec::Vec;

use crate::allocator::zm;
use crate::prelude::*;
use crate::mem::{AVirtRange, Allocation, PageLayout};
use crate::consts;

use super::VirtAddrSpace;

struct ProcessMapping {
    map_range: AVirtRange,
    allocation: Allocation,
}

pub struct ProcessAddrSpace {
    addr_space: VirtAddrSpace,
    mappings: Vec<ProcessMapping>,
}

impl ProcessAddrSpace {
    pub fn new() -> KResult<ProcessAddrSpace> {
        Ok(ProcessAddrSpace {
            addr_space: VirtAddrSpace::new()?,
            mappings: Vec::new(),
        })
    }

    pub fn map(&mut self, addr: VirtAddr, size: usize) -> KResult<()> {
        let allocation = zm().alloc(PageLayout::from_size_align(size, 4096).ok_or(SysErr::InvlArgs)?);
    }

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

    fn insert_mapping(
        &mut self,
        mapping: ProcessMapping,
    ) -> KResult<()> {
        let insert_index = self.get_mapping_insert_index(mapping.map_range)
            .ok_or(SysErr::InvlMemZone)?;

        self.mappings.insert(insert_index, mapping);

        Ok(())
    }

    fn remove_mapping_from_address(&mut self, address: VirtAddr) -> Option<ProcessMapping> {
        let mapping = self.mappings.remove(
            self.get_mapping_index(address)?,
        );

        Some(mapping)
    }
}

impl Drop for ProcessAddrSpace {
    fn drop(&mut self) {
        // safety: address space will not be loaded, because thread holds strong reference to address space
        // it means all threads with this address space have been dropped when this is dropped
        unsafe {
            self.addr_space.dealloc_addr_space()
        }
    }
}