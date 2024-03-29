use derive_more::{Add, AddAssign, Sub, SubAssign, Mul, MulAssign, Div, DivAssign};
use serde::{Serialize, Deserialize};
use bytemuck::{Zeroable, Pod};

use crate::{PAGE_SIZE, page_aligned, align_up};

#[repr(transparent)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Add, AddAssign, Sub, SubAssign, Mul, MulAssign, Div, DivAssign, Serialize, Deserialize, Zeroable, Pod)]
pub struct Size(usize);

impl Size {
    pub const fn from_bytes(bytes: usize) -> Self {
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
        // try is not allowed in const
        if let Some(bytes) = pages.checked_mul(PAGE_SIZE) {
            Some(Size(bytes))
        } else {
            None
        }
    }

    pub const fn zero() -> Self {
        Size(0)
    }

    pub const fn bytes(self) -> usize {
        self.0
    }

    pub const fn bytes_aligned(self) -> usize {
        align_up(self.0, PAGE_SIZE)
    }

    pub const fn as_aligned(self) -> Self {
        Size(self.bytes_aligned())
    }

    pub fn pages(self) -> Option<usize> {
        if page_aligned(self.0) {
            Some(self.0 / PAGE_SIZE)
        } else {
            None
        }
    }

    pub const fn pages_rounded(self) -> usize {
        self.bytes_aligned() / PAGE_SIZE
    }

    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }

    pub fn is_page_aligned(self) -> bool {
        page_aligned(self.0)
    }
}