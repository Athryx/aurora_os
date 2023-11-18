use core::fmt;

use serde::{Serialize, Deserialize};

/// Result type returned by syscalls
pub type KResult<T> = Result<T, SysErr>;

/// Error codes returned by syscalls
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(usize)]
pub enum SysErr {
    Ok = 0,
    OkUnreach = 1,
    OkTimeout = 2,
    OutOfMem = 3,
    InvlId = 4,
    InvlPerm = 5,
    InvlWeak = 6,
    InvlArgs = 7,
    InvlOp = 8,
    InvlMemZone = 9,
    InvlVirtAddr = 10,
    InvlPhysAddr = 11,
    InvlAlign = 12,
    Overflow = 13,
    OutOfCapacity = 14,
    InvlBytecode = 15,
    Obscured = 16,
    InvlSyscall = 17,
    Unknown = 18,
}

impl SysErr {
    /// Creates a SysErr from the given number, returns none if `n` is an invalid syserr code
    pub fn new(n: usize) -> Option<Self> {
        if n > Self::Unknown as usize {
            None
        } else {
            unsafe { Some(core::mem::transmute(n)) }
        }
    }

    pub const fn num(&self) -> usize {
        *self as usize
    }

    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Ok => "no error",
            Self::OkUnreach => "no waiting thread to act upon",
            Self::OkTimeout => "operation timed out",
            Self::OutOfMem => "out of memory",
            Self::InvlId => "invalid identifier",
            Self::InvlPerm => "invalid capability permissions",
            Self::InvlWeak => "weak capability referenced dead object",
            Self::InvlArgs => "invalid arguments",
            Self::InvlOp => "invalid operation",
            Self::InvlMemZone => "invalid memory zone or memory zone collision",
            Self::InvlVirtAddr => "non canonical address",
            Self::InvlPhysAddr => "invalid physical address",
            Self::InvlAlign => "invalid alignment",
            Self::Overflow => "supplied syscall values caused an integer overflow or underflow",
            Self::OutOfCapacity => "not enough capacity available in some container",
            Self::InvlBytecode => "invalid bytecode",
            Self::Obscured => "operation does not return information about error state",
            Self::InvlSyscall => "invalid syscall number",
            Self::Unknown => "unknown error",
        }
    }
}

impl fmt::Display for SysErr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}