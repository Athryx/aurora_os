use thiserror_no_std::Error;
use sys::SysErr;

use crate::allocator::addr_space::AddrSpaceError;
use executor::Executor;

mod executor;
mod task;

#[derive(Debug, Error)]
pub enum AsyncError {
    #[error("An error occured while trying to map memory: {0}")]
    MapError(#[from] AddrSpaceError),
    #[error("A system error occured: {0}")]
    SysErr(#[from] SysErr),
}

crate::thread_local! {
    pub static EXECUTOR: Executor = Executor::new().expect("failed to initialize async executor");
}