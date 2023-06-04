#![no_std]

use sys::Process;

use sync::Once;

mod addr_space_manager;
mod alloc;
mod sync;

static THIS_PROCESS: Once<Process> = Once::new();

fn this_process() -> &'static Process {
    THIS_PROCESS.get().unwrap()
}

pub fn init() {}