#![no_std]

#![feature(associated_type_defaults)]
#![feature(decl_macro)]
#![feature(trait_alias)]

extern crate alloc;

pub mod env;
pub mod fs;
pub mod prelude;
pub mod process;
pub mod service;

pub use aurora_core::{thread, allocator, sync, collections};
pub use aurora_core::{this_context, addr_space};
pub use sys::{dprint, dprintln};