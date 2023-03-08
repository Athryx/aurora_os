//! The sys crate is a low level interface to the aurora kernel syscalls
#![no_std]

pub mod syscall_nums;

mod cap;
pub use cap::*;
mod syserr;
pub use syserr::SysErr;