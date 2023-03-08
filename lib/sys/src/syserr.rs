/// Error codes returned by syscalls
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
    InvlAlign = 11,
    Obscured = 12,
    InvlSyscall = 13,
    Unknown = 14,
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
            Self::InvlAlign => "invalid alignment",
            Self::Obscured => "operation does not return information about error state",
            Self::InvlSyscall => "invalid syscall number",
            Self::Unknown => "unknown error",
        }
    }
}