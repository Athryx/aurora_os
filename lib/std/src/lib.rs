#![no_std]
#![feature(lang_items)]
#![feature(naked_functions)]

// needed to get global allocator working
extern crate aurora;

pub mod prelude;

mod rt;
mod panicking;
mod startup;

pub use core::*;
pub use alloc::*;