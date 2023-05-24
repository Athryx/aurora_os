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
use crate::alloc::{PaRef, HeapRef};
use page_table::{PageTable, PageTablePointer, PageTableFlags};

//mod frame_mapper;
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
#[derive(Debug, Clone, Copy)]
struct PageMappingTaker {
    virt_range: AVirtRange,
    phys_range: APhysRange,
}

impl PageMappingTaker {
    fn get_take_size(&self) -> Option<PageSize> {
        Some(core::cmp::min(
            self.phys_range.get_take_size()?,
            self.virt_range.get_take_size()?,
        ))
    }

    fn peek(&self) -> Option<(PhysFrame, VirtFrame)> {
        let take_size = self.get_take_size()?;

        Some((
            PhysFrame::new(self.phys_range.addr(), take_size),
            VirtFrame::new(self.virt_range.addr(), take_size),
        ))
    }

    fn take(&mut self) -> Option<(PhysFrame, VirtFrame)> {
        let take_size = self.get_take_size()?;

        Some((
            self.phys_range.take(take_size)?,
            self.virt_range.take(take_size)?,
        ))
    }
}

/// Represents a memory zone that has been mapped in the page tables
#[derive(Debug, Clone, Copy)]
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
    pub fn new(mut page_allocator: PaRef, alloc_ref: HeapRef) -> KResult<Self> {
        let pml4_table = PageTable::new(&mut page_allocator, PageTableFlags::NONE)
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
                    .expect(FAIL_MESSAGE)
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
                .dealloc_all(&mut self.page_allocator)
        }
    }

    /// Maps the given virtual memory range to point to the given physical address
    /// 
    /// Will return InvlArgs if `flags` does not specify either read, write, or execute
    pub fn map_memory(&mut self, virt_range: AVirtRange, phys_addr: PhysAddr, flags: PageMappingFlags) -> KResult<()> {
        if !flags.exists() {
            return Err(SysErr::InvlArgs);
        }

        self.add_virt_addr_entry(virt_range, phys_addr, flags)?;

        let result = self.map_memory_inner(virt_range, phys_addr, flags, false);

        if result.is_err() {
            self.remove_virt_addr_entry(virt_range.addr()).unwrap();
        }

        result
    }

    /// Maps all the virt ranges to the given physical address by repeatedly calling map_memory
    pub fn map_many<T: Iterator<Item = (AVirtRange, PhysAddr)> + Clone>(&mut self, iter: T, flags: PageMappingFlags) -> KResult<()> {
        let iter_copy = iter.clone();

        for (i, (virt_range, phys_addr)) in iter.enumerate() {
            let result = self.map_memory(virt_range, phys_addr, flags);

            if result.is_err() {
                for (virt_range, _) in iter_copy.take(i) {
                    self.unmap_memory(virt_range).unwrap();
                }
                return result;
            }
        }

        Ok(())
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

        let page_taker = PageMappingTaker {
            virt_range,
            phys_range,
        };

        let result = self.map_memory_from_page_taker(page_taker, flags, global);

        if result.is_err() {
            // TODO: handle out of memory condition more elegantly
            self.unmap_memory_inner(virt_range, phys_addr);
        }

        result
    }

    fn map_memory_from_page_taker(
        &mut self,
        mut page_taker: PageMappingTaker,
        flags: PageMappingFlags,
        global: bool,
    ) -> KResult<()> {
        let page_taker_copy = page_taker;

        while let Some((phys_frame, virt_frame)) = page_taker.take() {
            if let Err(error) = self.map_frame(virt_frame, phys_frame, flags, global) {
                self.unmap_memory_from_page_taker(page_taker_copy);

                return Err(error);
            }

            // TODO: check if address space is loaded
            invlpg(virt_frame.start_addr().as_usize());
        }

        Ok(())
    }

    /// Resizes the memory mapping starting at the addres of `new_mapping_range`
    /// to have the size of `new_mapping_range`
    // TODO: split this function up into smaller functions
    pub fn resize_mapping(&mut self, new_mapping_range: AVirtRange) -> KResult<()> {
        let old_mapping = self.resize_virt_addr_entry(new_mapping_range)?;

        let phys_addr = old_mapping.phys_addr;
        let flags = old_mapping.mapping_flags;

        let old_mapping_range = AVirtRange::new(new_mapping_range.addr(), old_mapping.virt_range.size());

        if old_mapping_range.size() == new_mapping_range.size() {
            // size is not changing, do nothing
            return Ok(())
        }

        let mut old_frame_taker = PageMappingTaker {
            virt_range: old_mapping_range,
            phys_range: APhysRange::new(phys_addr, old_mapping_range.size()),
        };

        let mut new_frame_taker = PageMappingTaker {
            virt_range: new_mapping_range,
            phys_range: APhysRange::new(phys_addr, new_mapping_range.size()),
        };

        // using the old and new frame takers, keep getting frames until they don't match
        // then unmap all the different zones from the old_frame_taker
        // and map the zones form the new_frame_taker
        // TODO: don't use this easier approach since it is a bit slower
        loop {
            if old_frame_taker.peek() != new_frame_taker.peek() {
                break;
            }

            old_frame_taker.take();
            new_frame_taker.take();
        }

        let result: KResult<()> = try {
            if let Some(old_frame) = old_frame_taker.peek()
                && let Some(new_frame) = new_frame_taker.peek() {
                if old_frame.1.get_size() > new_frame.1.get_size() {
                    // we will unmap some pages with 1 overlap
                    let depth = old_frame.1.get_size().page_table_depth();
                    let mut new_page_table = PageTable::new(&mut self.page_allocator, *PARENT_FLAGS)
                        .ok_or(SysErr::OutOfMem)?;

                    // map in the new table first
                    while let Some((phys_frame, virt_frame)) = new_frame_taker.take() {
                        if let Err(error) = self.map_page_table(
                            new_page_table,
                            depth,
                            virt_frame,
                            phys_frame,
                            flags,
                            false
                        ) {
                            unsafe {
                                new_page_table.as_mut_ptr()
                                    .as_mut()
                                    .unwrap()
                                    .dealloc_all(&mut self.page_allocator);
                            }
            
                            Err(error)?
                        }
                    }

                    // panic safety: shouldn't fail because we are replacing a table with an already existing parent
                    self.map_page_table_inner(
                        self.cr3,
                        0,
                        old_frame.1,
                        new_page_table,
                    ).unwrap();

                    invlpg(old_frame.1.start_addr().as_usize());

                    // unmap remaining pages
                    old_frame_taker.take();
                    self.unmap_memory_from_page_taker(old_frame_taker);
                } else {
                    // we will map some pages with 1 overlap
                    // panic safety: we know this page table exists because of addr entries
                    let old_page_table = self.get_page_table(new_frame.1)
                        .expect("memory resize error");

                    if let Err(error) = self.map_memory_from_page_taker(new_frame_taker, flags, false) {
                        // panic safety: no oom should occcur because no allocations are necassary
                        self.map_page_table_inner(
                            self.cr3,
                            0,
                            new_frame.1,
                            old_page_table,
                        ).expect("memory resize error");

                        // TODO: check if address space is loaded
                        invlpg(new_frame.1.start_addr().as_usize());

                        // take the frame that we restored above
                        new_frame_taker.take();
                        
                        // unmap the rest
                        self.unmap_memory_from_page_taker(new_frame_taker);

                        Err(error)?
                    }
                }
            } else if new_frame_taker.peek().is_none() {
                // no new pages need to be mapped, just unmap old pages
                self.unmap_memory_from_page_taker(old_frame_taker);
            } else if old_frame_taker.peek().is_none() {
                // no pages need to be unmapped, just map new pages
                self.map_memory_from_page_taker(new_frame_taker, flags, false)?
            }
            // no changes need to be made otherwise
        };

        if result.is_err() {
            // reset virt addr entry if mapping failed
            // panic safety: since this was mapped before, it should still be available
            self.resize_virt_addr_entry(old_mapping_range).unwrap();
        }

        result
    }

    /// Unmaps all the virtual memory ranges in the slice
    /// 
    /// Phys addr must be the same memory it was mapped with
    /// 
    /// If any one of the memeory regions fails, none will be unmapped
    pub fn unmap_memory(&mut self, virt_range: AVirtRange) -> KResult<()> {
        let phys_addr = self.remove_virt_addr_entry(virt_range.addr())?.phys_addr;

        self.unmap_memory_inner(virt_range, phys_addr);

        Ok(())
    }

    /// Same as [`unmap_memory`] but it doesn't modify the virtual adress entries
    fn unmap_memory_inner(&mut self, virt_range: AVirtRange, phys_addr: PhysAddr) {
        let phys_range = APhysRange::new(phys_addr, virt_range.size());

        let page_taker = PageMappingTaker {
            virt_range,
            phys_range,
        };

        self.unmap_memory_from_page_taker(page_taker);
    }

    fn unmap_memory_from_page_taker(&mut self, mut page_taker: PageMappingTaker) {
        while let Some((_, virt_frame)) = page_taker.take() {
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

    /// Resizes a mapped zone, and returns the old value
    fn resize_virt_addr_entry(&mut self, virt_range: AVirtRange) -> KResult<MappedZone> {
        // can't map anything beyond the kernel region
        if virt_range.end_usize() > *consts::KERNEL_VMA {
            return Err(SysErr::InvlMemZone);
        }

        let index = self.get_mapped_range_by_addr(virt_range.addr())
            .ok_or(SysErr::InvlMemZone)?;

        if index == self.mem_zones.len() - 1 || virt_range.end_addr() <= self.mem_zones[index + 1].virt_range.addr() {
            let mapping = self.mem_zones[index];
            self.mem_zones[index].virt_range = virt_range;
            Ok(mapping)
        } else {
            Err(SysErr::InvlMemZone)
        }
    }

    /// No longer marks the given range starting at virt_addr as mapped
    /// 
    /// Returns the zones corresponding mapping
    fn remove_virt_addr_entry(&mut self, virt_addr: VirtAddr) -> KResult<MappedZone> {
        let index = self.get_mapped_range_by_addr(virt_addr)
            .ok_or(SysErr::InvlMemZone)?;

        Ok(self.mem_zones.remove(index))
    }

    /// Gets the page table at the given address and level of `virt_frame`
    fn get_page_table(&self, virt_frame: VirtFrame) -> Option<PageTablePointer> {
        let virt_addr = virt_frame.start_addr().as_usize();
        let page_table_indicies = [
            get_bits(virt_addr, 39..48),
			get_bits(virt_addr, 30..39),
			get_bits(virt_addr, 21..30),
			get_bits(virt_addr, 12..21),
        ];

        let depth = virt_frame.get_size().page_table_depth();

        let mut page_table_pointer = self.cr3;

        for level in 0..depth {
            let index = page_table_indicies[level];

            if !page_table_pointer.is_page_table() {
                return None;
            }

            // safety: we have checked that page table pointer is a page table
            let page_table = unsafe {
                page_table_pointer.as_mut_ptr().as_mut().unwrap()
            };

            let Some(new_page_table) = page_table.get_page_table_pointer(index) else { return None };
            page_table_pointer = new_page_table;
        }

        Some(page_table_pointer)
    }

    fn map_frame(&mut self,
        virt_frame: VirtFrame,
        phys_frame: PhysFrame,
        flags: PageMappingFlags,
        global: bool
    ) -> KResult<()> {
        self.map_page_table(self.cr3, 0, virt_frame, phys_frame, flags, global)
    }

    fn map_page_table(&mut self,
        base_table: PageTablePointer,
        starting_depth: usize,
        virt_frame: VirtFrame,
        phys_frame: PhysFrame,
        flags: PageMappingFlags,
        global: bool
    ) -> KResult<()> {
        let huge_flag = match virt_frame {
            VirtFrame::K4(_) => PageTableFlags::NONE,
            VirtFrame::M2(_) => PageTableFlags::HUGE,
            VirtFrame::G1(_) => PageTableFlags::HUGE,
        };

        let global_flag = if global {
            PageTableFlags::GLOBAL
        } else {
            PageTableFlags::NONE
        };

        let flags = PageTableFlags::PRESENT | huge_flag | global_flag | flags.into();
        self.map_page_table_inner(
            base_table,
            starting_depth,
            virt_frame,
            PageTablePointer::new(phys_frame.start_addr(), flags),
        )
    }

    fn map_page_table_inner(
        &mut self,
        mut base_table: PageTablePointer,
        starting_depth: usize,
        virt_frame: VirtFrame,
        page_table_pointer: PageTablePointer,
    ) -> KResult<()> {
        let virt_addr = virt_frame.start_addr().as_usize();
        let page_table_indicies = [
            get_bits(virt_addr, 39..48),
			get_bits(virt_addr, 30..39),
			get_bits(virt_addr, 21..30),
			get_bits(virt_addr, 12..21),
        ];

        let depth = virt_frame.get_size().page_table_depth();
        assert!(starting_depth < depth);

        let mut page_table = unsafe {
            base_table.as_mut_ptr().as_mut().unwrap()
        };

        for level in starting_depth..depth {
            let index = page_table_indicies[level];

            if level == depth - 1 {
                page_table.add_entry(index, page_table_pointer);
            } else {
                page_table = page_table
                    .get_or_alloc(index, &mut self.page_allocator, *PARENT_FLAGS)
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

        let depth = virt_frame.get_size().page_table_depth();

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
                    table.dealloc(&mut self.page_allocator)
                } else {
                    break;
                }
            }
        }
    }
}