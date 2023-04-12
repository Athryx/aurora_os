//! This has all the functions that have to do with mappind physical memory into virtual memory

use bitflags::bitflags;
use lazy_static::lazy_static;
use spin::Once;

use crate::arch::x64::invlpg;
use crate::cap::CapFlags;
use crate::mem::PageSize;
use crate::mem::PhysFrame;
use crate::mem::VirtFrame;
use crate::prelude::*;
use crate::consts;
use crate::alloc::{PaRef, AllocRef};
use page_table::{PageTable, PageTablePointer};

use self::page_table::PageTableFlags;

mod page_table;

lazy_static! {
    /// Most permissive page table flags used by parent tables
    static ref PARENT_FLAGS: PageTableFlags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::USER;
}

/// Cached page table pointer of kernel memory region
static KERNEL_MEMORY_PAGE_POINTER: Once<PageTablePointer> = Once::new();

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
    pub fn exists(&self) -> bool {
		self.intersects(PageMappingFlags::READ | PageMappingFlags::WRITE | PageMappingFlags::EXEC)
	}
}

impl From<CapFlags> for PageMappingFlags {
    fn from(flags: CapFlags) -> Self {
        let mut out = PageMappingFlags::USER;

        if flags.contains(CapFlags::READ) {
            out |= PageMappingFlags::READ;
        }

        if flags.contains(CapFlags::WRITE) {
            out |= PageMappingFlags::WRITE;
        }

        if flags.contains(CapFlags::PROD) {
            out |= PageMappingFlags::EXEC;
        }

        out
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

/// Represents a memory zone that has been mapped in the page tables
#[derive(Debug)]
struct MappedZone {
    virt_range: AVirtRange,
    phys_addr: PhysAddr,
    mapping_flags: PageMappingFlags,
}

/// This represents a virtual address space that can have memory mapped into it
#[derive(Debug)]
pub struct VirtAddrSpace {
    /// All virtual memory which is currently mapped
    // TODO: write btreemap for this, it will be faster with many zones
    mem_zones: Vec<MappedZone>,
    /// Page table pointer which will go in the cr3 register, it points to the pml4 table
    cr3: PageTablePointer,
    /// Page allocator used to allocate page frames for page tables
    page_allocator: PaRef,
}

impl VirtAddrSpace {
    pub fn new(mut page_allocator: PaRef, alloc_ref: AllocRef) -> KResult<Self> {
        let pml4_table = PageTable::new(page_allocator.allocator(), PageTableFlags::NONE)
            .ok_or(SysErr::OutOfMem)?;

        let mut out = VirtAddrSpace {
            mem_zones: Vec::new(alloc_ref),
            cr3: pml4_table,
            page_allocator,
        };

        out.initialize_kernel_mapping();

        Ok(out)
    }

    /// Sets up the kernel memory mapping
    /// 
    /// Kernel memory is the last 512 GiB of the virtual address space
    /// 
    /// Will mark kernel .text as executable, .rodata as read only, everything else as read and write
    /// The global flag will also be set because this mapping is shared between all processess
    /// 
    /// This is only sets up the mapping once, then the page table pointer is cached for future use
    fn initialize_kernel_mapping(&mut self) {
        const FAIL_MESSAGE: &'static str = "Failed to initialize kernel memory page tables";

        if let Some(page_table_pointer) = KERNEL_MEMORY_PAGE_POINTER.get() {
            // safety: cr3 is not referenced anywhere else because we have a mutable reference to self
            unsafe {
                self.cr3.as_mut_ptr()
                    .as_mut()
                    .unwrap()
                    .add_entry(511, *page_table_pointer);
            }
        } else {
            let text_phys_addr = consts::TEXT_VIRT_RANGE.addr().to_phys();
            self.map_memory_inner(
                *consts::TEXT_VIRT_RANGE,
                text_phys_addr,
                PageMappingFlags::READ | PageMappingFlags::EXEC,
                true,
            ).expect(FAIL_MESSAGE);

            let rodata_phys_addr = consts::RODATA_VIRT_RANGE.addr().to_phys();
            self.map_memory_inner(
                *consts::RODATA_VIRT_RANGE,
                rodata_phys_addr,
                PageMappingFlags::READ,
                true,
            ).expect(FAIL_MESSAGE);

            // this is to fix an issue since the last address in KERNEL_VIRTUAL_MEMORY is 2^64 - 1,
            // so end_addr() of KERNEL_VIRTUAL_MEMORY (which is called by split_at_iter) will cause an overflow
            // we just reduce the size of KERNEL_VIRTUAL_MEMORY by the size of the largest frame,
            // then map that one seperately
            let max_frame_size = PageSize::MAX_SIZE as usize;
            let mem_region = AVirtRange::new_aligned(
                VirtAddr::new(*consts::KERNEL_VMA),
                (usize::MAX - *consts::KERNEL_VMA) - 1 - max_frame_size,
            );

            let mem_iter = mem_region
                .split_at_iter(*consts::TEXT_VIRT_RANGE)
                .flat_map(|range| range.split_at_iter(*consts::RODATA_VIRT_RANGE));

            for mem_range in mem_iter {
                let mem_phys_addr = mem_range.addr().to_phys();
                self.map_memory_inner(
                    mem_range,
                    mem_phys_addr,
                    PageMappingFlags::READ | PageMappingFlags::WRITE,
                    true,
                ).expect(FAIL_MESSAGE);
            }

            // map the last frame
            let last_phys_frame = PhysFrame::G1(PhysAddr::new(mem_region.size()));
            let last_virt_frame = VirtFrame::G1(mem_region.end_addr());
            self.map_frame(
                last_virt_frame,
                last_phys_frame,
                PageMappingFlags::READ | PageMappingFlags::WRITE,
                true,
            ).expect(FAIL_MESSAGE);

            // safety: cr3 is not referenced anywhere else because we have a mutable reference to self
            let kernel_page_pointer = unsafe {
                self.cr3.as_mut_ptr()
                    .as_mut()
                    .unwrap()
                    .get_page_table_pointer(511)
            };

            KERNEL_MEMORY_PAGE_POINTER.call_once(|| kernel_page_pointer);
        }
    }

    pub fn cr3_addr(&self) -> PhysAddr {
        self.cr3.address()
    }

    /// Deallocates all the page tables in this address space
    /// 
    /// Call this before dropping the address space otherwise there will be a memory leak
    /// 
    /// # Safety
    /// 
    /// This address space must not be loaded when this is called
    pub unsafe fn dealloc_addr_space(&mut self) {
        unsafe {
            self.cr3.as_mut_ptr().as_mut().unwrap()
                .dealloc_all(self.page_allocator.allocator())
        }
    }

    /// Maps all the virtual memory ranges in the slice to point to the corresponding physical address
    /// 
    /// If any one of the memeory regions fails, none will be mapped
    /// 
    /// Will return InvlArgs if `flags` does not specify either read, write, or execute
    pub fn map_memory(&mut self, virt_range: AVirtRange, phys_addr: PhysAddr, flags: PageMappingFlags) -> KResult<()> {
        if !flags.exists() {
            return Err(SysErr::InvlArgs);
        }

        self.add_virt_addr_entry(virt_range, phys_addr, flags)?;

        let result = self.map_memory_inner(virt_range, phys_addr, flags, false);

        if result.is_err() {
            self.remove_virt_addr_entry(virt_range).unwrap();
        }

        result
    }

    /// Same as [`map_memory`] but doesn't make the zones as in use
    /// 
    /// This is used when creating the kernel mappings
    /// 
    /// `global` specifies if the mapping should be global
    fn map_memory_inner(
        &mut self,
        virt_range: AVirtRange,
        phys_addr: PhysAddr,
        flags: PageMappingFlags,
        global: bool,
    ) -> KResult<()> {
        let phys_range = APhysRange::new(phys_addr, virt_range.size());

        let mut frame_taker = PageMappingTaker {
            virt_range,
            phys_range,
        };

        while let Some((phys_frame, virt_frame)) = frame_taker.take() {
            // TODO: handle out of memory condition more elegantly
            if let Err(error) = self.map_frame(virt_frame, phys_frame, flags, global) {
                self.unmap_memory_inner(virt_range, phys_addr);

                return Err(error);
            }

            // TODO: check if address space is loaded
            invlpg(virt_frame.start_addr().as_usize());
        }

        Ok(())
    }

    /// Resizes the memory mapping starting at the addres of `new_mapping_range`
    /// to have the size of `new_mapping_range`
    pub fn resize_mapping(&mut self, new_mapping_range: AVirtRange) -> KResult<()> {
        let mapping_index = self.get_mapped_range_by_addr(new_mapping_range.addr())
            .ok_or(SysErr::InvlMemZone)?;

        let old_size = self.mem_zones[mapping_index].virt_range.size();
        let new_size = new_mapping_range.size();
    }

    /// Unmaps all the virtual memory ranges in the slice
    /// 
    /// Phys addr must be the same memory it was mapped with
    /// 
    /// If any one of the memeory regions fails, none will be unmapped
    pub fn unmap_memory(&mut self, virt_range: AVirtRange) -> KResult<()> {
        let phys_addr = self.remove_virt_addr_entry(virt_range)?;

        self.unmap_memory_inner(virt_range, phys_addr);

        Ok(())
    }

    /// Same as [`unmap_memory`] but it doesn't modify the virtual adress entries
    fn unmap_memory_inner(&mut self, virt_range: AVirtRange, phys_addr: PhysAddr) {
        let phys_range = APhysRange::new(phys_addr, virt_range.size());

        let mut frame_taker = PageMappingTaker {
            virt_range,
            phys_range,
        };

        while let Some((_, virt_frame)) = frame_taker.take() {
            self.unmap_frame(virt_frame);

            // TODO: check if address space is loaded
            invlpg(virt_frame.start_addr().as_usize());
        }
    }

    /// Returns Some(index) if the given virt range in the virtual address space is not occupied
    /// 
    /// The index is the place where the virt_range can be inserted to maintain ordering in the list
    fn virt_range_unoccupied(&self, virt_range: AVirtRange) -> Option<usize> {
        // can't map anything beyond the kernel region
        if virt_range.end_usize() > *consts::KERNEL_VMA {
            return None;
        }

        match self.mem_zones.binary_search_by_key(&virt_range.addr(), |mapping| mapping.virt_range.addr()) {
            // If we find the address it is occupied
            Ok(_) => None,
            Err(index) => {
                if (index == 0 || self.mem_zones[index - 1].virt_range.end_addr() <= virt_range.addr())
                    && (index == self.mem_zones.len() || virt_range.end_addr() <= self.mem_zones[index].virt_range.addr()) {
                    Some(index)
                } else {
                    None
                }
            },
        }
    }

    /// Returns the index of the mapped range that starts at the give address, or none if no such range exists
    fn get_mapped_range_by_addr(&self, virt_addr: VirtAddr) -> Option<usize> {
        self.mem_zones
            .binary_search_by_key(&virt_addr, |mapping| mapping.virt_range.addr())
            .ok()
    }

    /// Marks the virt_range as mapped to phys_addr, so no future mappings will overwrite this data
    fn add_virt_addr_entry(
        &mut self,
        virt_range: AVirtRange,
        phys_addr: PhysAddr,
        mapping_flags: PageMappingFlags,
    ) -> KResult<()> {
        let index = self.virt_range_unoccupied(virt_range).ok_or(SysErr::InvlMemZone)?;
        
        let new_mapping = MappedZone {
            virt_range,
            phys_addr,
            mapping_flags,
        };

        self.mem_zones.insert(index, new_mapping).or(Err(SysErr::OutOfMem))
    }

    /// No longer marks the given virt_range as mapped
    /// 
    /// Returns the physical address the the range's mapping started at
    fn remove_virt_addr_entry(&mut self, virt_range: AVirtRange) -> KResult<PhysAddr> {
        let index = self.get_mapped_range_by_addr(virt_range.addr())
            .ok_or(SysErr::InvlMemZone)?;

        Ok(self.mem_zones.remove(index).phys_addr)
    }

    fn map_frame(&mut self, virt_frame: VirtFrame, phys_frame: PhysFrame, flags: PageMappingFlags, global: bool) -> KResult<()> {
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

        let global_flag = if global {
            PageTableFlags::GLOBAL
        } else {
            PageTableFlags::NONE
        };

        let mut page_table = unsafe {
            self.cr3.as_mut_ptr().as_mut().unwrap()
        };

        for level in 0..depth {
            let index = page_table_indicies[level];

            if level == depth - 1 {
                let flags = PageTableFlags::PRESENT | huge_flag | global_flag | flags.into();
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

        // the index of the first entry in tables that needs to be deallocated
        let mut dealloc_start_index = depth;

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
                dealloc_start_index = depth;
            } else {
                // don't continue removing this page table unless we have deallocated this table 
                break;
            }
        }

        // dealloc these in a later pass after all indexes are removed
        for i in dealloc_start_index..depth {
            unsafe {
                if let Some(table) = tables[i].as_mut() {
                    table.dealloc(self.page_allocator.allocator())
                } else {
                    break;
                }
            }
        }
    }
}