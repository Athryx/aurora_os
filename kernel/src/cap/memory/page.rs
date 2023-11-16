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
        let slice = self.allocation.as_mut_slice_ptr();

        unsafe {
            // TODO: figure out if this might need to be volatile
            // safety: caller must ensure that this memory capability only stores userspace data expecting to be written to
            ptr::write_bytes(slice.as_mut_ptr(), 0, slice.len());
        }
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