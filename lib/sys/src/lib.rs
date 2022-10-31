//! Crate for constants related to aurora kernel system calls
#![no_std]

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
    // unused for now, but will be used in future
    InvlPtr = 12,
    ResLimit = 13,
    Obscured = 14,
    Unknown = 15,
}

impl SysErr {
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
            Self::InvlPtr => "invalid pointer",
            Self::ResLimit => "operation could not be performed due to a resource limit",
            Self::Obscured => "operation does not return information about error state",
            Self::Unknown => "unknown error",
        }
    }
}

/// Aurora kernel syscall numbers
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum SysNums {
    PrintDebug = 0,

    ProcessNew,
    ProcessExit,
    ThreadNew,
    ThreadBlock,
    ProcessBindExcept,
    CapClone,
    CapMove,
    CapDestroy,
    WeakIsAlive,

    MemMap,
    MemUnmap,
    MemReserve,
    MemUnreserve,
    MemNew,
    MmioNew,
    MemSize,

    EventNew,
    EventArgc,
    EventSend,
    EventListen,
    EventNblisten,
    EventAlisten,
    EventAabort,
    Eret,

    ChannelNew,
    ChannelMsgProps,
    ChannelSend,
    ChannelRecv,
    ChannelNbsend,
    ChannelNbrecv,
    ChannelAsend,
    ChannelArecv,
    ChannelReplyRecv,
    ChannelCall,
    ChannelAcall,

    KeyNew,
    KeyId,

    IntNew,
    IntVector,
    IntBind,
    IntEoi,

    PortNew,
    PortNum,
    PortMap,
    PortUnmap,

    SpawnerNew,
    SpawnerKillAll,

    AllocatorNew,
    AllocatorCapacity,
    AllocatorPrealloc,
    AllocatorBindOomHandler,
    AllocatorSetMaxPages,

    RootOomListen,
    RootOomPanic,
}
