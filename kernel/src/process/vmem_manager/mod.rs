//! This has all the functions that have to do with mappind physical memory into virtual memory

use bitflags::bitflags;
use lazy_static::lazy_static;

use crate::arch::x64::get_cr3;
use crate::arch::x64::invlpg;
use crate::arch::x64::set_cr3;
use crate::mem::PhysFrame;
use crate::mem::VirtFrame;
use crate::prelude::*;
use crate::consts;
use crate::sync::IMutex;
use crate::sync::IMutexGuard;
use crate::alloc::{PaRef, AllocRef};
use page_table::{PageTable, PageTablePointer};

use self::page_table::PageTableFlags;

mod page_table;

lazy_static! {
	static ref HIGHER_HALF_PAGE_POINTER: PageTablePointer = PageTablePointer::new(*consts::KZONE_PAGE_TABLE_POINTER,
		PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::GLOBAL);
    
    /// Most permissive page table flags used by parent tables
    static ref PARENT_FLAGS: PageTableFlags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::USER;
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

/// Use to take a large as possible page size for use with huge pages
struct PageMappingTaker {
    virt_range: AVirtRange,
    phys_range: APhysRange,
}

impl PageMappingTaker {
    fn take(&mut self) -> Option<(PhysFrame, VirtFrame)> {
        let take_size = core::cmp::min(
            self.phys_range.get_take_size()?,
            self.virt_range.get_take_size()?,
        );

        Some((
            self.phys_range.take(take_size)?,
            self.virt_range.take(take_size)?,
        ))
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
        // can't map anything beyond the kernel region
        if virt_range.end_usize() > *consts::KERNEL_VMA {
            return None;
        }

        match self.mem_zones.binary_search_by_key(&virt_range.addr(), AVirtRange::addr) {
            // If we find the address it is occupied
            Ok(_) => None,
            Err(index) => {
                if (index == 0 || self.mem_zones[index - 1].end_addr() <= virt_range.addr())
                    && (index == self.mem_zones.len() || virt_range.end_addr() <= self.mem_zones[index].addr()) {
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

    fn remove_virt_addr_entries(&mut self, memory: &[(AVirtRange, PhysAddr)]) -> KResult<()> {
        // TODO: figure out if this atomic removing is even necessary, we might not need 2 passess

        for (virt_range, _) in memory {
            let index = self.mem_zones
                .binary_search_by_key(&virt_range.addr(), AVirtRange::addr)
                .map_err(|_| SysErr::InvlMemZone)?;
            
            if self.mem_zones[index] != *virt_range {
                return Err(SysErr::InvlMemZone);
            }
        }

        for (virt_range, _) in memory {
            let index = self.mem_zones
                .binary_search_by_key(&virt_range.addr(), AVirtRange::addr)
                .map_err(|_| SysErr::InvlMemZone)?;
            
            self.mem_zones.remove(index);
        }

        Ok(())
    }

    fn map_frame(&mut self, virt_frame: VirtFrame, phys_frame: PhysFrame, flags: PageMappingFlags) -> KResult<()> {
        let virt_addr = virt_frame.start_addr().as_usize();
        let page_table_indicies = [
            get_bits(virt_addr, 39..48),
			get_bits(virt_addr, 30..39),
			get_bits(virt_addr, 21..30),
			get_bits(virt_addr, 12..21),
        ];

        let (depth, huge_flag) = match virt_frame {
            VirtFrame::K4(_) => (4, PageTableFlags::NONE),
            VirtFrame::M2(_) => (3, PageTableFlags::HUGE),
            VirtFrame::G1(_) => (2, PageTableFlags::HUGE),
        };

        let mut page_table = unsafe {
            self.cr3.as_mut_ptr().as_mut().unwrap()
        };

        for level in 0..depth {
            let index = page_table_indicies[level];

            if level == depth - 1 {
                let flags = PageTableFlags::PRESENT | huge_flag | flags.into();
                page_table.add_entry(index, PageTablePointer::new(phys_frame.start_addr(), flags));
            } else {
                page_table = page_table
                    .get_or_alloc(index, self.page_allocator.allocator(), *PARENT_FLAGS)
                    .ok_or(SysErr::OutOfMem)?;
            }
        }

        Ok(())
    }

    /// Unmaps the given virtual memory frame
    /// 
    /// This function still works even if the frame isn't fully mapped, it will try and remove and partially mapped parent tables
    fn unmap_frame(&mut self, virt_frame: VirtFrame) {
        let virt_addr = virt_frame.start_addr().as_usize();
        let page_table_indicies = [
            get_bits(virt_addr, 39..48),
			get_bits(virt_addr, 30..39),
			get_bits(virt_addr, 21..30),
			get_bits(virt_addr, 12..21),
        ];

        let depth = match virt_frame {
            VirtFrame::K4(_) => 4,
            VirtFrame::M2(_) => 3,
            VirtFrame::G1(_) => 2,
        };

        let mut tables = [self.cr3.as_mut_ptr(), null_mut(), null_mut(), null_mut()];

        for a in 1..depth {
            unsafe {
                tables[a] = if let Some(page_table) = tables[a - 1].as_mut() {
                    page_table.get(page_table_indicies[a - 1])
                } else {
                    break
                };
            }
        }

        for i in (0..depth).rev() {
            let current_table = unsafe {
                if let Some(table) = tables[i].as_mut() {
                    table
                } else {
                    continue;
                }
            };

            current_table.remove(page_table_indicies[i]);

            if i != 0 && current_table.entry_count() == 0 {
                unsafe {
                    current_table.dealloc(self.page_allocator.allocator());
                }
            }
        }
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
            pml4_table.as_mut_ptr()
                .as_mut()
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

    /// Loads this virtual address space
    pub unsafe fn load(&self) {
        set_cr3(self.cr3_addr.as_usize());
    }

    pub fn is_loaded(&self) -> bool {
        get_cr3() == self.cr3_addr.as_usize()
    }

    pub fn get_cr3_addr(&self) -> PhysAddr {
        self.cr3_addr
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
            inner.cr3.as_mut_ptr().as_mut().unwrap()
                .dealloc_all(inner.page_allocator.allocator())
        }
    }

    /// Maps all the virtual memory ranges in the slice to point to the corresponding physical address
    /// 
    /// If any one of the memeory regions fails, none will be mapped
    pub fn map_memory(&self, memory: &[(AVirtRange, PhysAddr)], flags: PageMappingFlags) -> KResult<()> {
        let mut inner = self.inner.lock();

        inner.add_virt_addr_entries(memory)?;

        for (virt_range, phys_addr) in memory {
            let phys_range = APhysRange::new(*phys_addr, virt_range.size());

            let mut frame_taker = PageMappingTaker {
                virt_range: *virt_range,
                phys_range,
            };

            while let Some((phys_frame, virt_frame)) = frame_taker.take() {
                // TODO: handle out of memory condition more elegantly
                if let Err(error) = inner.map_frame(virt_frame, phys_frame, flags) {
                    inner.remove_virt_addr_entries(memory).unwrap();

                    Self::unmap_internal(inner, memory);

                    return Err(error);
                }

                // TODO: check if address space is loaded
                invlpg(virt_frame.start_addr().as_usize());
            }
        }

        Ok(())
    }

    /// Unmaps all the virtual memory ranges in the slice
    /// 
    /// Phys addr must be the same memory it was mapped with
    /// 
    /// If any one of the memeory regions fails, none will be unmapped
    // FIXME: don't require phys addr to be passed in
    pub fn unmap_memory(&self, memory: &[(AVirtRange, PhysAddr)]) -> KResult<()> {
        let mut inner = self.inner.lock();

        inner.remove_virt_addr_entries(memory)?;

        Self::unmap_internal(inner, memory);

        Ok(())
    }

    fn unmap_internal(mut inner: IMutexGuard<VirtAddrSpaceInner>, memory: &[(AVirtRange, PhysAddr)]) {
        for (virt_range, phys_addr) in memory {
            let phys_range = APhysRange::new(*phys_addr, virt_range.size());

            let mut frame_taker = PageMappingTaker {
                virt_range: *virt_range,
                phys_range,
            };

            while let Some((_, virt_frame)) = frame_taker.take() {
                inner.unmap_frame(virt_frame);

                // TODO: check if address space is loaded
                invlpg(virt_frame.start_addr().as_usize());
            }
        }
    }
}