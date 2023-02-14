pub mod kernel_stack;
mod thread;
mod thread_map;

use spin::Once;
pub use thread::Thread;
use thread_map::ThreadMap;

use crate::prelude::*;
use crate::sync::{IMutex, IMutexGuard};

static THREAD_MAP: Once<IMutex<ThreadMap>> = Once::new();

pub fn thread_map() -> IMutexGuard<'static, ThreadMap> {
    THREAD_MAP.get().expect("thread map not initilized").lock()
}

/// Called every time the local apic timer ticks
pub fn timer_handler() {
}

pub fn init() -> KResult<()> {
    let mut thread_map = ThreadMap::new();
    thread_map.ensure_cpu()?;

    THREAD_MAP.call_once(|| IMutex::new(thread_map));

    Ok(())
}

pub fn ap_init(stack_addr: usize) -> KResult<()> {
    thread_map().ensure_cpu()?;
    
    Ok(())
}