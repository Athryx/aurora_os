use crate::alloc::PaRef;
use crate::prelude::*;
use crate::mem::{PageSize, PhysFrame, VirtFrame};
use crate::process::vmem_manager::page_table;
use super::PageMappingFlags;
use super::page_table::{PageTable, PageTablePointer, PageTableFlags, NUM_ENTRIES};

const PAGE_TABLE_DEPTH: usize = 4;

lazy_static! {
    /// Most permissive page table flags used by parent tables
    static ref PARENT_FLAGS: PageTableFlags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::USER;
}

/// Manages a page table and can map memory inside of it
#[derive(Debug)]
pub struct FrameMapper {
    /// The address of the start of the region that this FrameMapper manages
    /// 
    /// This must be correctly aligned for the page_level of this frame manager
    start_addr: VirtAddr,
    /// What level of page table this frame mapper manages
    page_level: usize,
    page_table: Option<PageTablePointer>,
    allocator: PaRef,
}

impl FrameMapper {
    /// Creates a new frame mapper managing the memory starting at `start_addr`
    /// 
    /// # Panics
    /// 
    /// if level is larger than the maximum page table level
    pub fn new(start_addr: VirtAddr, level: usize, allocator: PaRef) -> FrameMapper {
        assert!(level <= PAGE_TABLE_DEPTH);

        let out = FrameMapper {
            start_addr,
            page_level: level,
            page_table: None,
            allocator,
        };

        match out.level_size() {
            Some(size) => assert!(align_of(start_addr.as_usize()) >= size),
            None => assert!(start_addr.as_usize() == 0),
        }

        out
    }

    /// Returns the size of the memory this frame mapper manages, or none if it manages the whole address space
    pub fn level_size(&self) -> Option<usize> {
        match self.page_level {
            0 => None,
            1 => Some(PAGE_SIZE * NUM_ENTRIES * NUM_ENTRIES * NUM_ENTRIES),
            2 => Some(PAGE_SIZE * NUM_ENTRIES * NUM_ENTRIES),
            3 => Some(PAGE_SIZE * NUM_ENTRIES),
            4 => Some(PAGE_SIZE),
            _ => unreachable!(),
        }
    }

    pub fn contains(&self, virt_frame: VirtFrame) -> bool {
        let start_addr = self.start_addr.as_usize();
        let other_addr = virt_frame.start_addr().as_usize();

        let (mask, max_page_size) = match self.page_level {
            0 => return true,
            1 => (0xffffff8000000000, PageSize::G1),
            2 => (0xffffffffc0000000, PageSize::G1),
            3 => (0xffffffffffe00000, PageSize::M2),
            4 => (0xfffffffffffff000, PageSize::K4),
            _ => unreachable!(),
        };

        (virt_frame.get_size() <= max_page_size) && (mask & start_addr) == (mask & other_addr)
    }

    /// Maps the given frame, there must be no overlap with other mapped regions
    fn map_frame(&mut self, virt_frame: VirtFrame, phys_frame: PhysFrame, flags: PageMappingFlags, global: bool) -> KResult<()> {
        assert!(self.contains(virt_frame));

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

        let final_page_flags = PageTableFlags::PRESENT | huge_flag | global_flag | flags.into();

        // handle case where the page takes up entire region we manage
        if self.page_level == depth {
            assert!(self.page_table.is_none());
            self.page_table = Some(PageTablePointer::new(phys_frame.start_addr(), final_page_flags));
            return Ok(());
        }

        let mut page_table_pointer = match self.page_table {
            Some(page_table_pointer) => page_table_pointer,
            None => {
                let page_table_pointer = PageTable::new(self.allocator.allocator(), *PARENT_FLAGS)
                    .ok_or(SysErr::OutOfMem)?;

                self.page_table = Some(page_table_pointer);
                page_table_pointer
            }
        };

        assert!(page_table_pointer.is_page_table());

        let mut page_table = unsafe {
            page_table_pointer.as_mut_ptr().as_mut().unwrap()
        };

        for level in self.page_level..depth {
            let index = page_table_indicies[level];

            if level == depth - 1 {
                let flags = PageTableFlags::PRESENT | huge_flag | global_flag | flags.into();
                page_table.add_entry(index, PageTablePointer::new(phys_frame.start_addr(), flags));
            } else {
                page_table = page_table
                    .get_or_alloc(index, self.allocator.allocator(), *PARENT_FLAGS)
                    .ok_or(SysErr::OutOfMem)?;
            }
        }

        Ok(())
    }

    /// Unmaps the given virtual memory frame
    /// 
    /// This function still works even if the frame isn't fully mapped, it will try and remove and partially mapped parent tables
    fn unmap_frame(&mut self, virt_frame: VirtFrame) {
        assert!(self.contains(virt_frame));

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

        let Some(page_table_pointer) = self.page_table;

        let mut tables = [page_table_pointer.as_mut_ptr(), null_mut(), null_mut(), null_mut()];

        if depth == self.page_level {
            assert!(!page_table_pointer.is_page_table());
            self.page_table = None;
        }

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