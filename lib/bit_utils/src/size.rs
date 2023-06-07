use core::ops::Mul;

use derive_more::{Add, Sub, Mul, Div};

use crate::{PAGE_SIZE, page_aligned, align_up};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Add, Sub, Mul, Div)]
pub struct Size(usize);

impl Size {
    pub fn from_bytes(bytes: usize) -> Self {
        Size(bytes)
    }

    /// Creates a size from the given pages
    /// 
    /// # Panics
    /// 
    /// panics if the size in pages is larger than usize::MAX when converted to bytes
    pub const fn from_pages(pages: usize) -> Self {
        Self::try_from_pages(pages)
            .expect("overflow occured when converting pages to bytes")
    }

    pub const fn try_from_pages(pages: usize) -> Option<Self> {
        Some(Size(pages.checked_mul(PAGE_SIZE)?))
    }

    pub const fn bytes(&self) -> usize {
        self.0
    }

    pub const fn bytes_aligned(&self) -> usize {
        align_up(self.0, PAGE_SIZE)
    }

    pub fn pages(&self) -> Option<usize> {
        if page_aligned(self.0) {
            Some(self.0 / PAGE_SIZE)
        } else {
            None
        }
    }

    pub const fn pages_rounded(&self) -> usize {
        self.bytes_aligned() / PAGE_SIZE
    }

    pub const fn is_zero(&self) -> bool {
        self.0 == 0
    }
}

/*impl Mul<usize> for Size {
    fn mul(self, rhs: usize) -> Self::Output {
        Size(self.0 * rhs)
    }
}*/