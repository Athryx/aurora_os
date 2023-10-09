use core::{cmp::min, alloc::Layout};

use bit_utils::Size;
use bytemuck::AnyBitPattern;

use crate::prelude::*;
use super::{MemoryInner, AllocationEntry};

/// Used to copy data into a memory capability
/// 
/// Users of `MemoryWriter` should repeatedly call [`write_region`]
pub trait MemoryWriter {
    /// Writes the given region into this writer
    fn write_region(&mut self, region: MemoryWriteRegion) -> WriteResult;
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
        Ok(writer.write_region(self.into()).write_size)
    }
}

/// Copies the bytes into memory directly
pub struct PlainMemoryWriter<'a> {
    pub(super) memory: &'a MemoryInner,
    /// current index of allocation entry
    pub(super) alloc_entry_index: usize,
    pub(super) offset: usize,
    pub(super) end_offset: usize,
}

impl PlainMemoryWriter<'_> {
    fn remaining_write_capacity(&self) -> usize {
        self.end_offset - self.offset
    }

    fn current_alloc_entry(&self) -> AllocationEntry {
        self.memory.allocations[self.alloc_entry_index]
    }

    fn current_offset_ptr(&self) -> *mut u8 {
        let dest_offset = self.offset - self.current_alloc_entry().offset;
        unsafe {
            self.current_alloc_entry().allocation.as_mut_ptr::<u8>().add(dest_offset)
        }
    }

    pub fn push_usize_ptr(&mut self) -> Option<*mut usize> {
        // number of bytes that need to be pushed for usize to be aligned
        let align_amount = align_up(self.offset, size_of::<usize>()) - self.offset;

        let bytes = [0u8; size_of::<usize>()];
        let write_slice = &bytes[..align_amount];
        if self.write_region(write_slice.into()).end_reached {
            return None;
        }

        // this pointer will now be aligned
        // since usize is size aligned, this will not span 2 pages, so it will be in a contigous allocation
        let out = self.current_offset_ptr() as *mut usize;

        if self.write_region(bytes.as_slice().into()).write_size.bytes() != size_of::<usize>() {
            None
        } else {
            Some(out)
        }
    }
}

impl MemoryWriter for PlainMemoryWriter<'_> {
    fn write_region(&mut self, region: MemoryWriteRegion) -> WriteResult {
        let mut src_offset = 0;

        let mut dest_offset = self.offset - self.current_alloc_entry().offset;

        // amount written in this call of write_region
        let mut amount_written = 0;

        loop {
            if self.remaining_write_capacity() == 0 {
                return WriteResult {
                    write_size: Size::from_bytes(amount_written),
                    end_reached: true,
                };
            }

            let mut allocation = self.current_alloc_entry().allocation;

            let write_size = min(region.size() - src_offset, allocation.size() - dest_offset);
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
                return WriteResult {
                    write_size: Size::from_bytes(amount_written),
                    end_reached: self.remaining_write_capacity() == 0,
                };
            } else {
                // finished consuming current alloc entry
                self.alloc_entry_index += 1;

                dest_offset = 0;
                src_offset += write_size;
            }
        }
    }
}