use aurora::allocator::addr_space::AddrSpaceError;
use thiserror_no_std::Error;

use arpc::RpcError;

#[derive(Debug, Error)]
pub enum FsError {
    #[error("An rpc error occured: {0}")]
    RpcError(#[from] RpcError),
    #[error("An address space error occured: {0}")]
    AddrSpaceError(#[from] AddrSpaceError),
    #[error("Could not access memory mapped io for storage device")]
    DeviceMapError,
}