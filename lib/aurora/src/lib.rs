#![no_std]

#![feature(try_blocks)]
#![feature(let_chains)]
#![feature(nonnull_slice_from_raw_parts)]
#![feature(slice_ptr_get)]
#![feature(slice_take)]

extern crate alloc;

use addr_space_manager::AddrSpaceManager;
use context::Context;
use sync::{Once, Mutex, MutexGuard};

mod addr_space_manager;
mod aser;
mod allocator;
mod context;
mod prelude;
mod sync;

static THIS_CONTEXT: Once<Context> = Once::new();

pub fn this_context() -> &'static Context {
    THIS_CONTEXT.get().unwrap()
}

static ADDR_SPACE: Once<Mutex<AddrSpaceManager>> = Once::new();

pub fn addr_space() -> MutexGuard<'static, AddrSpaceManager> {
    ADDR_SPACE.get().unwrap().lock()
}

pub fn init() {}