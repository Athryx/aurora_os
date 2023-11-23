//! Flags use by some aurora kernel syscalls

use bitflags::bitflags;

use crate::CapFlags;

bitflags! {
    /// Flags that are used when moving and copying capabilties
    #[derive(Debug, Clone, Copy)]
    pub struct CapCloneFlags: u32 {
        const READ = 1;
        const PROD = 1 << 1;
        const WRITE = 1 << 2;
        const UPGRADE = 1 << 3;
        /// If true, MAKE_WEAK flag is used to determine if strong or weak cap is made
        /// 
        /// If not set, capability weakness remains unchanged
        const CHANGE_CAP_WEAKNESS = 1 << 4;
        /// If true, a weak capability is made, otherwise a strong capability is made
        /// 
        /// Has no effect if CHANGE_CAP_WEAKNESS is not set
        const MAKE_WEAK = 1 << 5;
        /// If true, the old capability is destroyed and only the new one remains
        const DESTROY_SRC_CAP = 1 << 6;
        /// The src process is the current process
        const SRC_CSPACE_SELF = 1 << 7;
        /// The dst process is the current process
        const DST_CSPACE_SELF = 1 << 8;
    }
}

bitflags! {
    #[derive(Debug, Clone, Copy)]
    pub struct CapDestroyFlags: u32 {
        /// Destroy the capability from the current process rather than the target process passed in
        const CSPACE_SELF = 1;
    }
}

impl From<CapCloneFlags> for CapFlags {
    fn from(value: CapCloneFlags) -> Self {
        let mut out = CapFlags::empty();

        if value.contains(CapCloneFlags::READ) {
            out |= CapFlags::READ;
        }

        if value.contains(CapCloneFlags::PROD) {
            out |= CapFlags::PROD;
        }

        if value.contains(CapCloneFlags::WRITE) {
            out |= CapFlags::WRITE;
        }

        if value.contains(CapCloneFlags::UPGRADE) {
            out |= CapFlags::UPGRADE;
        }

        out
    }
}


bitflags! {
    #[derive(Debug, Clone, Copy)]
    pub struct HandleEventSyncFlags: u32 {
        const TIMEOUT = 1;
    }
}

bitflags! {
    #[derive(Debug, Clone, Copy)]
    pub struct HandleEventAsyncFlags: u32 {
        const AUTO_REQUE = 1;
    }
}


bitflags! {
    #[derive(Debug, Clone, Copy)]
    pub struct ThreadNewFlags: u32 {
        const CREATE_CAPABILITY_SPACE = 1;
        const THREAD_AUTOSTART = 1 << 1;
    }
}

bitflags! {
    #[derive(Debug, Clone, Copy)]
    pub struct ThreadSuspendFlags: u32 {
        const SUSPEND_TIMEOUT = 1;
    }
}

bitflags! {
    #[derive(Debug, Clone, Copy)]
    pub struct ThreadDestroyFlags: u32 {
        const DESTROY_OTHER = 1;
    }
}


bitflags! {
    /// Used to specify access permissions on memory mappings
    #[derive(Debug, Clone, Copy)]
    pub struct MemoryMappingFlags: u32 {
        const READ = 1;
        const WRITE = 1 << 1;
        const EXEC = 1 << 2;
    }
}

impl Default for MemoryMappingFlags {
    fn default() -> Self {
        Self::READ | Self::WRITE
    }
}

bitflags! {
    #[derive(Debug, Clone, Copy)]
    pub struct MemoryNewFlags: u32 {
        /// Memory will be allocated when it is first accessed
        const LAZY_ALLOC = 1;
        /// Memory will be zeroed
        const ZEROED = 1 << 1;
    }
}

bitflags! {
    /// The first three bits of flags are the same as MemoryMappingFlags, additonal options are here
    #[derive(Debug, Clone, Copy)]
    pub struct MemoryMapFlags: u32 {
        const MAX_SIZE = 1 << 3;
    }
}

bitflags! {
    #[derive(Debug, Clone, Copy)]
    pub struct MemoryUpdateMappingFlags: u32 {
        // first 3 bits are used by memory mapping flags
        const UPDATE_SIZE = 1 << 3;
        const EXACT_SIZE = 1 << 4;
        const UPDATE_FLAGS = 1 << 5;
    }
}

bitflags! {
    /// Used by memory_resize syscall
    #[derive(Debug, Clone, Copy)]
    pub struct MemoryResizeFlags: u32 {
        /// New memory will be allocated when it is first accessed
        const LAZY_ALLOC = 1;
        /// New memory will be zeroed
        const ZEROED = 1 << 1;
        /// Allows resizing memory if it is only mapped once
        const IN_PLACE = 1 << 2;
        /// If memory is increased in size, and in place is specified,
        /// the in place mapping is increased to the end of the memory capability
        const GROW_MAPPING = 1 << 3;
    }
}

bitflags! {
    /// Used by event_pool_await syscall
    #[derive(Debug, Clone, Copy)]
    pub struct EventPoolAwaitFlags: u32 {
        const TIMEOUT = 1;
    }
}

bitflags! {
    /// Used by `chennel_sync_send` and `channel_sync_recv`
    #[derive(Debug, Clone, Copy)]
    pub struct ChannelSyncFlags: u32 {
        const TIMEOUT = 1;
    }
}

bitflags! {
    #[derive(Debug, Clone, Copy)]
    pub struct ChannelAsyncRecvFlags: u32 {
        const AUTO_REQUE = 1;
    }
}