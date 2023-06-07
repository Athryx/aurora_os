#![no_std]

#![feature(try_blocks)]

use addr_space_manager::AddrSpaceManager;
use context::Context;
use sync::{Once, Mutex, MutexGuard};

mod addr_space_manager;
mod alloc;
mod context;
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