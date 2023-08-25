//! Utility for iterating over arrays with different tag types that are commonly provided by multipboot and acpi

use core::iter::FusedIterator;
use core::marker::PhantomData;

use bytemuck::AnyBitPattern;
use bytemuck::checked::pod_read_unaligned;

use crate::prelude::*;

/// A struct with trailing bytes
#[derive(Debug, Clone, Copy)]
pub struct WithTrailer<'a, T> {
    pub data: T,
    pub trailer: &'a [u8],
}

/// A trait that allows constructing a WithTrailer<T> from a raw pointer
pub trait TrailerInit {
    /// The size of the data type and its trailer
    fn size(&self) -> usize;
}

impl<T: TrailerInit> WithTrailer<'_, T> {
    /// Creates a WithTrailer<T> from a raw pointer
    /// 
    /// # Safety
    /// 
    /// The pointer must be valid for writes of up to the size thet the TrailerInit::size reports
    pub unsafe fn from_pointer<'a>(data_ptr: *const T) -> WithTrailer<'a, T> {
        let header = unsafe { ptr::read_unaligned(data_ptr) };
        let size = header.size();

        let trailer_ptr = unsafe { data_ptr.add(1) as *const u8 };
        let trailer = unsafe { core::slice::from_raw_parts(trailer_ptr, size - size_of::<T>()) };

        WithTrailer {
            data: header,
            trailer,
        }
    }
}

/// A tag header for elements of certain lists, like mb2
pub trait HwaTag: AnyBitPattern {
    type Elem<'a>: core::fmt::Debug;

    /// returns the size of the element, including the tag
    fn size(&self) -> usize;

    /// Retrieves the element for this tag
    fn elem<'a>(this: WithTrailer<'a, Self>) -> Self::Elem<'a>;

    /// Conveniance function to get data for this tag
    fn data<T: AnyBitPattern>(this: &WithTrailer<Self>) -> T {
        pod_read_unaligned(&this.trailer[..size_of::<T>()])
    }

    /// Conveniance function to get data for this tag with a trailer
    fn data_trailer<'a, T: AnyBitPattern>(this: &WithTrailer<'a, Self>) -> WithTrailer<'a, T> {
        let data = pod_read_unaligned(&this.trailer[..size_of::<T>()]);
        WithTrailer {
            data,
            trailer: &this.trailer[size_of::<T>()..],
        }
    }
}

/// Hardware array iterator
/// Iterates over arrays of different sized elements with different type elements
pub struct HwaIter<'a, T: HwaTag> {
    /// Data where tags and elements are stored
    bytes: &'a [u8],
    /// Required alignment of elements
    align: usize,
    marker: PhantomData<T>,
}

impl<'a, T: HwaTag> HwaIter<'a, T> {
    pub fn from(bytes: &'a [u8]) -> Self {
        Self::from_align(bytes, 0)
    }

    pub fn from_align(bytes: &'a [u8], align: usize) -> Self {
        HwaIter {
            bytes, 
            align,
            marker: PhantomData,
        }
    }
}

impl<'a, T: HwaTag> Iterator for HwaIter<'a, T> {
    type Item = T::Elem<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.bytes.len() < size_of::<T>() {
            None
        } else {
            let tag: T = pod_read_unaligned(&self.bytes[..size_of::<T>()]);

            let size = tag.size();
            let trailer = if size > size_of::<T>() {
                &self.bytes[size_of::<T>()..size]
            } else {
                &[]
            };

            let tag_trailer = WithTrailer {
                data: tag,
                trailer,
            };

            let advance_size = if self.align > 0 {
                align_up(size, self.align)
            } else {
                size
            };

            self.bytes = &self.bytes[advance_size..];

            let out = T::elem(tag_trailer);

            Some(out)
        }
    }
}

impl<T: HwaTag> FusedIterator for HwaIter<'_, T> {}
