#![no_std]

#![feature(try_blocks)]

use sys::{Process, Allocator};

use sync::Once;

mod addr_space_manager;
mod alloc;
mod sync;

static THIS_PROCESS: Once<Process> = Once::new();

fn this_process() -> &'static Process {
    THIS_PROCESS.get().unwrap()
}

static ALLOCATOR: Once<Allocator> = Once::new();

fn allocator() -> &'static Allocator {
    ALLOCATOR.get().unwrap()
}

pub fn init() {}