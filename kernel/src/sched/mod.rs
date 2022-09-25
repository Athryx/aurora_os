mod stack;
mod thread;
mod thread_map;

use spin::Once;
pub use thread::Registers;
use thread::Thread;
use thread_map::ThreadMap;

use crate::prelude::*;
use crate::sync::IMutex;

static THREAD_MAP: Once<IMutex<ThreadMap>> = Once::new();

pub fn thread_map() -> &'static IMutex<ThreadMap> {
    THREAD_MAP.get().expect("thread map not initilized")
}

pub fn init() -> KResult<()> {
    let mut thread_map = ThreadMap::new();
    thread_map.ensure_cpu()?;

    THREAD_MAP.call_once(|| IMutex::new(thread_map));

    Ok(())
}
