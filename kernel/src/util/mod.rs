//! A collection of miscallaneous utility functions

use crate::alloc::HeapRef;
use crate::mem::{MemOwner, MemOwnerKernelExt};
use crate::prelude::*;

mod hwa_iter;
mod id;

pub use bit_utils::*;
use bytemuck::AnyBitPattern;
use bytemuck::checked::pod_read_unaligned;
pub use hwa_iter::*;

/// Moves `object` to the heap specified by `allocer`
pub fn to_heap<T>(object: T, allocer: &mut HeapRef) -> KResult<*mut T> {
    Ok(MemOwner::new(object, allocer)?.ptr_mut())
}

pub fn iter_unaligned_pod_data<T: AnyBitPattern>(data: &[u8]) -> impl Iterator<Item = T> + '_ {
    data.chunks_exact(size_of::<T>())
        .map(|chunk| pod_read_unaligned(chunk))
}