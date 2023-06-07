#![no_std]
#![feature(lang_items)]

// needed to get global allocator working
extern crate aurora;

pub mod prelude;

mod rt;
mod panicking;

pub use core::*;
pub use alloc::*;