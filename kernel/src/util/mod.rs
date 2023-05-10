//! A collection of miscallaneous utility functions

use crate::alloc::HeapRef;
use crate::mem::MemOwner;
use crate::prelude::*;

mod hwa_iter;
mod id;

pub use bit_utils::*;
pub use hwa_iter::*;

/// Moves `object` to the heap specified by `allocer`
pub fn to_heap<T>(object: T, allocer: &mut HeapRef) -> KResult<*mut T> {
    Ok(MemOwner::new(object, allocer)?.ptr_mut())
}