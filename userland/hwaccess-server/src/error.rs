use aurora::allocator::addr_space::AddrSpaceError;
use thiserror_no_std::Error;
use sys::SysErr;

#[derive(Debug, Error)]
pub enum HwAccessError {
    #[error("Could not allocate physical memory: {0}")]
    AllocPhysMemError(#[from] SysErr),
    #[error("Could not map physical mamory in address space: {0}")]
    MapPhysMemError(#[from] AddrSpaceError),
}