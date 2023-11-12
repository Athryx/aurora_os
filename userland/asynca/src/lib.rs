#![no_std]

#![feature(negative_impls)]

extern crate alloc;

use core::future::Future;

use thiserror_no_std::Error;
use sys::SysErr;
use aurora_core::allocator::addr_space::AddrSpaceError;

use executor::Executor;

use self::task::JoinHandle;

pub mod async_sys;
mod executor;
mod task;

#[derive(Debug, Error)]
pub enum AsyncError {
    #[error("An error occured while trying to map memory: {0}")]
    MapError(#[from] AddrSpaceError),
    #[error("A system error occured: {0}")]
    SysErr(#[from] SysErr),
}

aurora_core::thread_local! {
    pub static EXECUTOR: Executor = Executor::new().expect("failed to initialize async executor");
}

/// Runs the asynchronous task and blocks until it finishes
pub fn block_in_place<T: 'static>(task: impl Future<Output = T> + 'static) -> T {
    EXECUTOR.with(|executor| {
        let join_handle = executor.spawn(task);
        executor.run().expect("block in place: failed to run executor");

        join_handle.get_output()
    })
}

/// Spawns a new asyncrhonous task
pub fn spawn<T: 'static>(task: impl Future<Output = T> + 'static) -> JoinHandle<T> {
    EXECUTOR.with(|executor| {
        executor.spawn(task)
    })
}