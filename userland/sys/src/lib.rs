//! The sys crate is a low level interface to the aurora kernel syscalls
#![no_std]

pub mod syscall_nums;

mod cap;
pub use cap::*;
mod events;
pub use events::*;
mod flags;
pub use flags::*;
mod init_info;
pub use init_info::*;
mod ipc;
pub use ipc::*;
mod process_init_data;
pub use process_init_data::*;
mod syscalls;
pub use syscalls::*;
mod syserr;
pub use syserr::*;