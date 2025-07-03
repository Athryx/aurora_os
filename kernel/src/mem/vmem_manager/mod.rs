//! This has all the functions that have to do with mappind physical memory into virtual memory

use lazy_static::lazy_static;
use spin::Once;
use sys::CapFlags;
use sys::{MemoryCacheSetting, MemoryMappingFlags};

use crate::arch::x64::invlpg;
use crate::mem::PageSize;
use crate::mem::PhysFrame;
use crate::mem::VirtFrame;
use crate::prelude::*;
use crate::consts;
use crate::mem::PaRef;
use page_table::{PageTable, PageTablePointer, PageTableFlags};

mod page_table;

lazy_static! {
    /// Most permissive page table flags used by parent tables
    static ref PARENT_FLAGS: PageTableFlags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::USER;
}

/// Cached page table pointer of kernel memory region
static KERNEL_MEMORY_PAGE_POINTER: Once<PageTablePointer> = Once::new();

#[derive(Debug, Clone, Copy)]
pub struct PageMappingOptions {
    pub read: bool,
    pub write: bool,
    pub exec: bool,
    pub user: bool,
    pub cacheing: MemoryCacheSetting,
}

impl PageMappingOptions {
    pub fn writable(self, write: bool) -> Self {
        PageMappingOptions {
            write,
            ..self
        }
    }

    /// Returns true if these page mapping options specify memory that will actually exist in the address space
    pub fn exists(&self) -> bool {
        self.read || self.write || self.exec
    }

    /// Gets the required capability flags for a memory capability to map it with these mapping options
    pub fn required_cap_flags(&self) -> CapFlags {
        let mut out = CapFlags::empty();

        if self.read || self.exec {
            out |= CapFlags::READ;
        }

        if self.write {
            out |= CapFlags::WRITE;
        }

        out
    }
}

impl Default for PageMappingOptions {
    fn default() -> Self {
        PageMappingOptions {
            read: false,
            write: false,
            exec: false,
            user: true,
            cacheing: MemoryCacheSetting::default(),
        }
    }
}

impl From<MemoryMappingFlags> for PageMappingOptions {
    fn from(flags: MemoryMappingFlags) -> Self {
        PageMappingOptions {
            read: flags.contains(MemoryMappingFlags::READ),
            write: flags.contains(MemoryMappingFlags::WRITE),
            exec: flags.contains(MemoryMappingFlags::EXEC),
            user: true,
            cacheing: flags.into(),
        }
    }
}

/// This represents a virtual address space that can have memory mapped into it
#[derive(Debug)]
pub struct VirtAddrSpace {
    /// Page table pointer which will go in the cr3 register, it points to the pml4 table
    cr3: PageTablePointer,
    /// Page allocator used to allocate page frames for page tables
    page_allocator: PaRef,
}

impl VirtAddrSpace {
    pub fn new(mut page_allocator: PaRef) -> KResult<Self> {
        let pml4_table = PageTable::new(&mut page_allocator, PageTableFlags::empty())
            .ok_or(SysErr::OutOfMem)?;

        let mut out = VirtAddrSpace {
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
        const FAIL_MESSAGE: &str = "Failed to initialize kernel memory page tables";

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
            self.map_memory_with_huge_pages(
                *consts::TEXT_VIRT_RANGE,
                text_phys_addr,
                PageMappingOptions {
                    read: true,
                    exec: true,
                    ..Default::default()
                },
                true,
            ).expect(FAIL_MESSAGE);

            let rodata_phys_addr = consts::RODATA_VIRT_RANGE.addr().to_phys();
            self.map_memory_with_huge_pages(
                *consts::RODATA_VIRT_RANGE,
                rodata_phys_addr,
                PageMappingOptions {
                    read: true,
                    ..Default::default()
                },
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
                self.map_memory_with_huge_pages(
                    mem_range,
                    mem_phys_addr,
                    PageMappingOptions {
                        read: true,
                        write: true,
                        ..Default::default()
                    },
                    true,
                ).expect(FAIL_MESSAGE);
            }

            // map the last frame
            let last_phys_frame = PhysFrame::G1(PhysAddr::new(mem_region.size()));
            let last_virt_frame = VirtFrame::G1(mem_region.end_addr());
            self.map_frame(
                last_virt_frame,
                last_phys_frame,
                PageMappingOptions {
                    read: true,
                    write: true,
                    ..Default::default()
                },
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

    /// Maps the given virtual memory page to reference the given physical memory page
    /// 
    /// Will return InvlArgs if `flags` does not specify either read, write, or execute
    /// 
    /// # Safety
    /// 
    /// mapping should be correct, lots of things can get messed up if wrong thing is mapped
    pub unsafe fn map_page(&mut self, virt_addr: VirtAddr, phys_addr: PhysAddr, options: PageMappingOptions) -> KResult<()> {
        unsafe {
            self.map_page_inner(virt_addr, phys_addr, options, false)
        }
    }

    unsafe fn map_page_inner(
        &mut self,
        virt_addr: VirtAddr,
        phys_addr: PhysAddr,
        options: PageMappingOptions,
        global: bool
    ) -> KResult<()> {
        assert!(virt_addr.as_usize() < *consts::KERNEL_START);
        assert!(page_aligned(virt_addr.as_usize()));

        if !options.exists() {
            return Err(SysErr::InvlArgs);
        }

        let global_flag = if global {
            PageTableFlags::GLOBAL
        } else {
            PageTableFlags::empty()
        };

        let flags = PageTableFlags::PRESENT | global_flag | options.into();
        let new_page_pointer = PageTablePointer::new(phys_addr, flags);

        let virt_addr = virt_addr.as_usize();
        let page_table_indicies = [
            get_bits(virt_addr, 39..48),
            get_bits(virt_addr, 30..39),
            get_bits(virt_addr, 21..30),
            get_bits(virt_addr, 12..21),
        ];

        let mut page_table = unsafe {
            self.cr3.as_mut_ptr().as_mut().unwrap()
        };

        for level in 0..4 {
            let index = page_table_indicies[level];

            if level == 3 {
                // last level
                unsafe {
                    page_table.add_entry(index, new_page_pointer);
                }
            } else {
                page_table = page_table
                    .get_or_alloc(index, &mut self.page_allocator, *PARENT_FLAGS)
                    .ok_or(SysErr::OutOfMem)?;
            }
        }

        // TODO: check if address space is loaded
        invlpg(virt_addr);

        Ok(())
    }

    /// Unmaps the page at `virt_addr`, returning the physical address it was mapped to
    /// 
    /// If the page was not mapped, returns None
    pub unsafe fn unmap_page(&mut self, virt_addr: VirtAddr) -> Option<PhysAddr> {
        let virt_addr = virt_addr.as_usize();

        assert!(virt_addr < *consts::KERNEL_START);
        assert!(page_aligned(virt_addr));

        let page_table_indicies = [
            get_bits(virt_addr, 39..48),
            get_bits(virt_addr, 30..39),
            get_bits(virt_addr, 21..30),
            get_bits(virt_addr, 12..21),
        ];

        let mut tables = [self.cr3.as_mut_ptr(), null_mut(), null_mut(), null_mut()];

        for a in 1..4 {
            unsafe {
                tables[a] = if let Some(page_table) = tables[a - 1].as_mut() {
                    page_table.get(page_table_indicies[a - 1])
                } else {
                    return None;
                };
            }
        }

        // the index of the first entry in tables that needs to be deallocated
        let mut dealloc_start_index = 4;

        let mut out = None;
        for i in (0..4).rev() {
            let current_table = unsafe {
                if let Some(table) = tables[i].as_mut() {
                    if i == 3 {
                        // get addres of table we are unmapping
                        out = Some(VirtAddr::new(table.get(page_table_indicies[i]) as usize).to_phys());
                    }
                    table
                } else {
                    continue;
                }
            };

            current_table.remove(page_table_indicies[i]);

            if i != 0 && current_table.entry_count() == 0 {
                dealloc_start_index = i;
            } else {
                // don't continue removing this page table unless we have deallocated this table 
                break;
            }
        }

        // dealloc these in a later pass after all indexes are removed
        for i in dealloc_start_index..4 {
            unsafe {
                if let Some(table) = tables[i].as_mut() {
                    table.dealloc(&mut self.page_allocator)
                } else {
                    break;
                }
            }
        }

        out
    }
}

#[derive(Debug, Clone, Copy)]
pub struct MapAction {
    pub virt_addr: VirtAddr,
    pub phys_addr: PhysAddr,
    pub options: PageMappingOptions,
}

impl VirtAddrSpace {
    /// Maps all the virt address to the given physical address by repeatedly calling map_page
    /// 
    /// If map_page fails for any page, all pages which were already mapped will be unmapped
    pub unsafe fn map_many<T: Iterator<Item = MapAction> + Clone>(&mut self, iter: T) -> KResult<()> {
        let iter_copy = iter.clone();

        for (i, action) in iter.enumerate() {
            let result = unsafe { 
                self.map_page(action.virt_addr, action.phys_addr, action.options)
            };

            if result.is_err() {
                for action in iter_copy.take(i) {
                    unsafe {
                        self.unmap_page(action.virt_addr).unwrap();
                    }
                }
                return result;
            }
        }

        Ok(())
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

// All these are used only to set up kernel mapping
impl VirtAddrSpace {
    fn map_memory_with_huge_pages(
        &mut self,
        virt_range: AVirtRange,
        phys_addr: PhysAddr,
        options: PageMappingOptions,
        global: bool,
    ) -> KResult<()> {
        let phys_range = APhysRange::new(phys_addr, virt_range.size());

        let mut page_taker = PageMappingTaker {
            virt_range,
            phys_range,
        };

        while let Some((phys_frame, virt_frame)) = page_taker.take() {
            self.map_frame(virt_frame, phys_frame, options, global)?;

            // TODO: check if address space is loaded
            invlpg(virt_frame.start_addr().as_usize());
        }

        Ok(())
    }

    fn map_frame(&mut self,
        virt_frame: VirtFrame,
        phys_frame: PhysFrame,
        options: PageMappingOptions,
        global: bool
    ) -> KResult<()> {
        let huge_flag = match virt_frame {
            VirtFrame::K4(_) => PageTableFlags::empty(),
            VirtFrame::M2(_) => PageTableFlags::HUGE,
            VirtFrame::G1(_) => PageTableFlags::HUGE,
        };

        let global_flag = if global {
            PageTableFlags::GLOBAL
        } else {
            PageTableFlags::empty()
        };

        // FIXME: handle case where pat bit is set, it must be set in a different bit for huge table
        // not a big deal since this should never be called with non writeback caching
        let flags = PageTableFlags::PRESENT | huge_flag | global_flag | options.into();
        self.map_frame_inner(
            virt_frame,
            PageTablePointer::new(phys_frame.start_addr(), flags),
        )
    }

    fn map_frame_inner(
        &mut self,
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

        let mut page_table = unsafe {
            self.cr3.as_mut_ptr().as_mut().unwrap()
        };

        for level in 0..depth {
            let index = page_table_indicies[level];

            if level == depth - 1 {
                unsafe {
                    page_table.add_entry(index, page_table_pointer);
                }
            } else {
                page_table = page_table
                    .get_or_alloc(index, &mut self.page_allocator, *PARENT_FLAGS)
                    .ok_or(SysErr::OutOfMem)?;
            }
        }

        Ok(())
    }
}