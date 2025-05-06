use crate::alloc::PaRef;
use crate::prelude::*;
use crate::container::Arc;
use crate::mem::{Allocation, PageLayout};

#[derive(Debug)]
pub struct Page {
    // this allocation is made to be the size of 1 page
    allocation: Allocation,
    allocator: PaRef,
}

impl Page {
    /// Gets the physical address of this page
    pub fn phys_addr(&self) -> PhysAddr {
        self.allocation.addr().to_phys()
    }

    pub fn new(mut allocator: PaRef) -> KResult<Self> {
        let allocation = allocator.alloc(
            PageLayout::from_size_align(PAGE_SIZE, PAGE_SIZE).unwrap(),
        ).ok_or(SysErr::OutOfMem)?;

        Ok(Page {
            allocation,
            allocator,
        })
    }

    pub fn new_zeroed(allocator: PaRef) -> KResult<Self> {
        let mut page = Page::new(allocator)?;

        unsafe {
            page.zero();
        }

        Ok(page)
    }

    pub fn allocation(&self) -> Allocation {
        self.allocation
    }

    pub fn create_copy(&self, allocer: PaRef) -> KResult<Self> {
        let mut new_page = Page::new(allocer)?;

        unsafe {
            new_page.copy_from(self);
        }

        Ok(new_page)
    }

    pub unsafe fn copy_from(&mut self, other: &Page) {
        let this_ptr = self.allocation.as_mut_ptr::<u8>();
        let other_ptr = other.allocation.as_ptr::<u8>();

        unsafe {
            core::ptr::copy_nonoverlapping(other_ptr, this_ptr, PAGE_SIZE);
        }
    }

    pub unsafe fn zero(&mut self) {
        unsafe { self.allocation.zero() }
    }
}

impl Drop for Page {
    fn drop(&mut self) {
        unsafe {
            self.allocator.dealloc(self.allocation);
        }
    }
}

#[derive(Debug)]
pub enum PageData {
    Owned(Page),
    Cow(Arc<Page>),
    LazyAlloc,
    LazyZeroAlloc,
}

#[derive(Debug, Clone, Copy)]
pub enum PageSource {
    Owned,
    OwnedZeroed,
    LazyAlloc,
    LazyZeroAlloc,
}

impl PageSource {
    pub fn get_page_data(&self, allocator: &PaRef) -> KResult<PageData> {
        match self {
            PageSource::Owned => {
                let page = Page::new(allocator.clone())?;
                Ok(PageData::Owned(page))
            },
            PageSource::OwnedZeroed => {
                let page = Page::new_zeroed(allocator.clone())?;
                Ok(PageData::Owned(page))
            }
            PageSource::LazyAlloc => Ok(PageData::LazyAlloc),
            PageSource::LazyZeroAlloc => Ok(PageData::LazyZeroAlloc),
        }
    }
}

pub enum NewPageIter<'a> {
    Alloced {
        allocation: Allocation,
        allocator: &'a PaRef,
        offset: usize,
    },
    LazyAlloc {
        remaining_count: usize,
    },
    LazyAllocZeroed {
        remaining_count: usize,
    },
}

impl Iterator for NewPageIter<'_> {
    type Item = PageData;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Alloced {
                allocation,
                allocator,
                offset,
            } => {
                if *offset >= allocation.size() {
                    None
                } else {
                    let mut out_allocation = Allocation::new(
                        allocation.as_usize() + *offset,
                        PAGE_SIZE,
                    );
                    out_allocation.zindex = allocation.zindex;
                    *offset += PAGE_SIZE;
                    Some(PageData::Owned(Page {
                        allocation: out_allocation,
                        allocator: allocator.clone(),
                    }))
                }
            },
            Self::LazyAlloc { remaining_count } => {
                if *remaining_count > 0 {
                    *remaining_count -= 1;
                    Some(PageData::LazyAlloc)
                } else {
                    None
                }
            },
            Self::LazyAllocZeroed { remaining_count } => {
                if *remaining_count > 0 {
                    *remaining_count -= 1;
                    Some(PageData::LazyZeroAlloc)
                } else {
                    None
                }
            },
        }
    }
}

impl PageSource {
    pub fn create_pages<'a>(&self, page_count: usize, allocator: &'a mut PaRef) -> KResult<NewPageIter<'a>> {
        match self {
            Self::Owned => {
                Ok(NewPageIter::Alloced {
                    allocation: allocator.alloc(PageLayout::from_size_align(page_count * PAGE_SIZE, PAGE_SIZE).unwrap())
                        .ok_or(SysErr::OutOfMem)?,
                    allocator,
                    offset: 0,
                })
            },
            Self::OwnedZeroed => {
                let mut allocation = allocator.alloc(PageLayout::from_size_align(page_count * PAGE_SIZE, PAGE_SIZE).unwrap())
                    .ok_or(SysErr::OutOfMem)?;
                unsafe {
                    allocation.zero();
                }

                Ok(NewPageIter::Alloced {
                    allocation,
                    allocator,
                    offset: 0,
                })
            },
            Self::LazyAlloc => Ok(NewPageIter::LazyAlloc { remaining_count: page_count }),
            Self::LazyZeroAlloc => Ok(NewPageIter::LazyAllocZeroed { remaining_count: page_count }),
        }
    }
}