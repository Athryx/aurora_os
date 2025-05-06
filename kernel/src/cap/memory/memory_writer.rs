use core::{cmp::min, alloc::Layout};
use core::cell::RefCell;

use bit_utils::Size;
use bytemuck::AnyBitPattern;

use crate::prelude::*;
use super::{MemoryInner, Page};

/// Used to copy data into a memory capability
/// 
/// Users of `MemoryWriter` should repeatedly call [`write_region`]
pub trait MemoryWriter {
    /// Gets the current position of the write pointer
    fn current_ptr(&mut self) -> KResult<*mut u8>;

    /// Writes the given region into this writer
    fn write_region(&mut self, region: MemoryWriteRegion) -> KResult<WriteResult>;

    fn push_usize_ptr(&mut self) -> KResult<(Option<*mut usize>, Size)> {
        let current_offset = self.current_ptr()? as usize;

        // number of bytes that need to be pushed for usize to be aligned
        let align_amount = align_up(current_offset, size_of::<usize>()) - current_offset;

        let bytes = [0u8; size_of::<usize>()];
        let write_slice = &bytes[..align_amount];

        let padding_write_result = self.write_region(write_slice.into())?;

        if padding_write_result.end_reached {
            return Ok((None, padding_write_result.write_size));
        }

        // this pointer will now be aligned
        // since usize is size aligned, this will not span 2 pages, so it will be in a contigous allocation
        let out = self.current_ptr()? as *mut usize;

        let ptr_write_size = self.write_region(bytes.as_slice().into())?.write_size;
        if ptr_write_size.bytes() != size_of::<usize>() {
            Ok((None, padding_write_result.write_size + ptr_write_size))
        } else {
            Ok((Some(out), padding_write_result.write_size + ptr_write_size))
        }
    }
}

/// Represents result of calling [`write_region`]
#[derive(Debug, Clone, Copy)]
pub struct WriteResult {
    pub write_size: Size,
    pub end_reached: bool,
}

pub struct MemoryWriteRegion<'a> {
    region: UVirtRange,
    _marker: PhantomData<&'a [u8]>,
}

impl MemoryWriteRegion<'_> {
    /// # Safety
    /// 
    /// `range` must remain valid for duration of this write regions use
    // FIXME: make write regions be created directly in memory cap implementation
    pub unsafe fn from_vrange(range: UVirtRange) -> Self {
        MemoryWriteRegion {
            region: range,
            _marker: PhantomData,
        }
    }

    pub fn size(&self) -> usize {
        self.region.size()
    }

    pub fn ptr(&self) -> *const u8 {
        self.region.addr().as_ptr()
    }

    /// Takes `num_bytes` off of the front of this region
    fn take_bytes(&mut self, num_bytes: usize) {
        self.region
            .take_layout(Layout::from_size_align(num_bytes, 1).unwrap())
            .unwrap();
    }

    pub fn read_value<T: AnyBitPattern>(&mut self) -> Option<T> {
        if size_of::<T>() <= self.size() {
            // safety: this pointer is valid for reads of at least size_of<T>() bytes
            // and AnyBitPattern bound ensures the type will be valid for any bytes read
            let out = unsafe {
                ptr::read_unaligned(self.ptr() as *const T)
            };

            self.take_bytes(size_of::<T>());

            Some(out)
        } else {
            None
        }
    }

    /// Attempts to read `buffer.len()` bytes into `buffer`, returns the amount of bytes actually read
    pub fn read_bytes(&mut self, buffer: &mut [u8]) -> usize {
        let read_size = min(buffer.len(), self.size());

        // safety: since we are writing to a mutable buffer, we know it will not alias with anything else
        unsafe {
            ptr::copy_nonoverlapping(self.ptr(), buffer.as_mut_ptr(), read_size);
        }

        self.take_bytes(read_size);

        read_size
    }
}

impl<'a, T> From<&'a [T]> for MemoryWriteRegion<'a> {
    fn from(value: &[T]) -> Self {
        MemoryWriteRegion {
            region: value.into(),
            _marker: PhantomData,
        }
    }
}

/// An object that serves as a source for copying into a memory capability
pub trait MemoryCopySrc {
    /// Size that could be written from this src
    fn size(&self) -> usize;

    /// Returns the number of bytes written
    fn copy_to(&self, writer: &mut impl MemoryWriter) -> KResult<Size>;
}

impl MemoryCopySrc for [u8] {
    fn size(&self) -> usize {
        self.len()
    }

    fn copy_to(&self, writer: &mut impl MemoryWriter) -> KResult<Size> {
        Ok(writer.write_region(self.into())?.write_size)
    }
}

/// Copies the bytes into memory directly
pub struct PlainMemoryWriter<'a> {
    pub(super) memory: &'a mut MemoryInner,
    /// current index of allocation entry
    pub(super) page_index: usize,
    pub(super) offset: usize,
    pub(super) end_offset: usize,
}

impl PlainMemoryWriter<'_> {
    fn current_page_offset(&self) -> usize {
        self.offset % PAGE_SIZE
    }

    fn remaining_write_capacity(&self) -> usize {
        self.end_offset - self.offset
    }

    fn current_page(&mut self) -> KResult<&mut Page> {
        self.memory.get_page_for_writing(self.page_index)
    }
}

impl MemoryWriter for PlainMemoryWriter<'_> {
    fn current_ptr(&mut self) -> KResult<*mut u8> {
        unsafe {
            Ok(self.current_page()?.allocation().as_mut_ptr::<u8>().add(self.current_page_offset()))
        }
    }

    fn write_region(&mut self, region: MemoryWriteRegion) -> KResult<WriteResult> {
        let mut src_offset = 0;

        let mut dest_offset = self.current_page_offset();

        // amount written in this call of write_region
        let mut amount_written = 0;

        loop {
            if self.remaining_write_capacity() == 0 {
                return Ok(WriteResult {
                    write_size: Size::from_bytes(amount_written),
                    end_reached: true,
                });
            }

            let mut allocation = self.current_page()?.allocation();

            let write_size = min(region.size() - src_offset, PAGE_SIZE - dest_offset);
            let write_size = min(write_size, self.remaining_write_capacity());

            // when src offset is set, it is ensured to be less than the size of the zone
            let src_ptr = unsafe { region.ptr().add(src_offset) };
            // safety: allocation_index_of_offset already checks offset is valid
            let dest_ptr = unsafe { allocation.as_mut_ptr::<u8>().add(dest_offset) };

            // safety: caller must ensure that this memory capability only stores userspace data expecting to be written to
            // TODO: get a version of copy_nonoverlapping that is safe, because we don't really care
            // if data is copied wrong when overalapping, that is userspaces problem
            unsafe {
                ptr::copy(src_ptr, dest_ptr, write_size);
            }

            amount_written += write_size;
            self.offset += write_size;

            if src_offset + write_size == region.size() {
                // finished copying everything from memory zone
                return Ok(WriteResult {
                    write_size: Size::from_bytes(amount_written),
                    end_reached: self.remaining_write_capacity() == 0,
                });
            } else {
                // finished writing to current page
                self.page_index += 1;

                dest_offset = 0;
                src_offset += write_size;
            }
        }
    }
}

struct PlainMemoryCopySrcInner<'a>(PlainMemoryWriter<'a>);

impl PlainMemoryCopySrcInner<'_> {
    fn current_page(&mut self) -> KResult<&Page> {
        self.0.memory.get_page_for_reading(self.0.page_index)
    }

    fn size(&self) -> usize {
        self.0.remaining_write_capacity()
    }

    fn copy_to(&mut self, writer: &mut impl MemoryWriter) -> KResult<Size> {
        let mut write_size = Size::zero();

        while self.0.remaining_write_capacity() != 0 {
            let offset = self.0.current_page_offset();
            let region_size = min(self.0.remaining_write_capacity(), PAGE_SIZE - offset);

            let page = self.current_page()?;
            let copy_region = UVirtRange::new(page.allocation().addr() + offset, region_size);
            // safety: current_page ensures region is valid for reading
            let region = unsafe { MemoryWriteRegion::from_vrange(copy_region) };

            let result = writer.write_region(region)?;
            write_size += result.write_size;
            self.0.offset += result.write_size.bytes();
            if result.end_reached {
                break;
            }
        }

        Ok(write_size)
    }
}

pub struct PlainMemoryCopySrc<'a>(RefCell<PlainMemoryCopySrcInner<'a>>);

impl<'a> From<PlainMemoryWriter<'a>> for PlainMemoryCopySrc<'a> {
    fn from(value: PlainMemoryWriter<'a>) -> Self {
        PlainMemoryCopySrc(
            RefCell::new(PlainMemoryCopySrcInner(value)),
        )
    }
}

impl MemoryCopySrc for PlainMemoryCopySrc<'_> {
    fn size(&self) -> usize {
        self.0.borrow().size()
    }

    fn copy_to(&self, writer: &mut impl MemoryWriter) -> KResult<Size> {
        self.0.borrow_mut().copy_to(writer)
    }
}