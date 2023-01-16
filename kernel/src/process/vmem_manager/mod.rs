//! This has all the functions that have to do with mappind physical memory into virtual memory

use bitflags::bitflags;
use lazy_static::lazy_static;

use crate::prelude::*;
use crate::consts;
use crate::sync::IMutex;
use crate::{alloc::{PaRef, AllocRef}, mem::Allocation};
use page_table::{PageTable, PageTablePointer};

use self::page_table::PageTableFlags;

mod page_table;

lazy_static! {
	static ref HIGHER_HALF_PAGE_POINTER: PageTablePointer = PageTablePointer::new(*consts::KZONE_PAGE_TABLE_POINTER,
		PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::GLOBAL);
}

bitflags! {
    /// Flags that represent properties of the memory we want to map
	pub struct PageMappingFlags: usize {
		const NONE =		0;
		const READ =		1;
		const WRITE =		1 << 1;
		const EXEC =		1 << 2;
		const USER = 		1 << 3;
	}
}

impl PageMappingFlags {
    /// Returns true if these page mapping flags specift memory that will actually exist in the address space
    fn exists(&self) -> bool {
		self.intersects(PageMappingFlags::READ | PageMappingFlags::WRITE | PageMappingFlags::EXEC)
	}
}

/// Fields in virt addr space that need ot be mutated so they will all be behind a lock
struct VirtAddrSpaceInner {
    /// All virtual memory which is currently in use
    // TODO: write btreemap for this, it will be faster with many zones
    mem_zones: Vec<AVirtRange>,
    /// Page table pointer which will go in the cr3 register, it points to the pml4 table
    cr3: PageTablePointer,
    /// Page allocator used to allocate page frames for page tables
    page_allocator: PaRef,
}

impl VirtAddrSpaceInner {
    /// Returns Some(index) if the given virt range in the virtual address space is not occupied
    /// 
    /// The index is the place where the virt_range can be inserted to maintain ordering in the list
    fn virt_range_unoccupied(&self, virt_range: AVirtRange) -> Option<usize> {
        match self.mem_zones.binary_search_by_key(&virt_range.addr(), |range| range.addr()) {
            // If we find the address it is occupied
            Ok(_) => None,
            Err(index) => {
                if self.mem_zones.get(index - 1)?.end_addr() <= virt_range.addr()
                    && virt_range.end_addr() <= self.mem_zones.get(index)?.addr() {
                    Some(index)
                } else {
                    None
                }
            },
        }
    }

    // this takes in a slice with the phys addrs as well because that is what map_memory takes in
    fn add_virt_addr_entries(&mut self, memory: &[(AVirtRange, PhysAddr)]) -> KResult<()> {
        // this vector stores indexes where new mem zones were inserted
        // if there is a conflict, we can backtrack and remove all the prevoius ones that were inserted
        let mut new_mem_indexes = Vec::new(self.mem_zones.alloc_ref());

        let mut inserted_count = 0;

        let _: Option<()> = try {
            for (virt_range, _) in memory {
                let index = self.virt_range_unoccupied(*virt_range)?;
                new_mem_indexes.push(index).ok()?;
                self.mem_zones.insert(index, *virt_range).ok()?;

                inserted_count += 1;
            }

            return Ok(());
        };

        // if we exit the try block adding the new ranges failed, we have to undo the operation

        // if the inserted count is less, it means the last operation failed when inserting into the mem_zones vector,
        // so we just ignore the last indes in new_mem_indexes
        if inserted_count < new_mem_indexes.len() {
            new_mem_indexes.pop();
        }

        for index in new_mem_indexes.iter().rev() {
            self.mem_zones.remove(*index);
        }

        Err(SysErr::InvlMemZone)
    }

    fn remove_virt_addr_entries(&mut self, memory: &[AVirtRange]) -> KResult<()> {
        // TODO: figure out if this atomic removing is even necessary, we might not need 2 passess

        for virt_range in memory {
            let index = self.mem_zones
                .binary_search_by_key(&virt_range.addr(), |range| range.addr())
                .map_err(|_| SysErr::InvlMemZone)?;
            
            if self.mem_zones[index] != *virt_range {
                return Err(SysErr::InvlMemZone);
            }
        }

        for virt_range in memory {
            let index = self.mem_zones
                .binary_search_by_key(&virt_range.addr(), |range| range.addr())
                .map_err(|_| SysErr::InvlMemZone)?;
            
            self.mem_zones.remove(index);
        }

        Ok(())
    }
}

/// This represents a virtual address space that can have memory mapped into it
pub struct VirtAddrSpace {
    /// Fields which need to be mutated
    inner: IMutex<VirtAddrSpaceInner>,
    /// Addres of the top level page table pointer, so we can load out without locking
    cr3_addr: PhysAddr,
}

impl VirtAddrSpace {
    pub fn new(mut page_allocator: PaRef, alloc_ref: AllocRef) -> Option<Self> {
        let mut pml4_table = PageTable::new(page_allocator.allocator(), PageTableFlags::NONE)?;

        unsafe {
            pml4_table.as_mut()
                .unwrap()
                .add_entry(511, *HIGHER_HALF_PAGE_POINTER);
        }

        Some(VirtAddrSpace {
            inner: IMutex::new(VirtAddrSpaceInner {
                mem_zones: Vec::new(alloc_ref),
                cr3: pml4_table,
                page_allocator,
            }),
            cr3_addr: pml4_table.address(),
        })
    }

    /// Deallocates all the page tables in this address space
    /// 
    /// Call this before dropping the address space otherwise there will be a memory leak
    /// 
    /// # Safety
    /// 
    /// This address space must not be loaded when this is called
    pub unsafe fn dealloc_addr_space(&self) {
        let mut inner = self.inner.lock();
        unsafe {
            inner.cr3.as_mut().unwrap()
                .dealloc_all(inner.page_allocator.allocator())
        }
    }

    /// Maps all the virtual memory ranges in the slice to point to the corresponding physical address
    /// 
    /// If any one of the memeory regions fails, none will be mapped
    pub fn map_memory(&self, memory: &[(AVirtRange, PhysAddr)]) -> KResult<()> {
        let mut inner = self.inner.lock();

        inner.add_virt_addr_entries(memory)?;

        for (virt_range, phys_addr) in memory {
            
        }

        Ok(())
    }

    /// Unmaps all the virtual memory ranges in the slice
    /// 
    /// If any one of the memeory regions fails, none will be unmapped
    pub fn unmap_memory(&self, memory: &[AVirtRange]) -> KResult<()> {
        let mut inner = self.inner.lock();

        inner.remove_virt_addr_entries(memory)?;

        Ok(())
    }
}