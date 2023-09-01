//! Flags use by some aurora kernel syscalls

use bitflags::bitflags;

use crate::CapFlags;

bitflags! {
    /// Flags that are used when moving and copying capabilties
    pub struct CapCloneFlags: u32 {
        const READ = 1;
        const PROD = 1 << 1;
        const WRITE = 1 << 2;
        const UPGRADE = 1 << 3;
        /// If true, a weak capability is made, otherwise a strong capability is made
        const MAKE_WEAK = 1 << 4;
        /// If true, the old capability is destroyed and only the new one remains
        const DESTROY_SRC_CAP = 1 << 5;
        /// The src process is the current process
        const SRC_PROCESS_SELF = 1 << 6;
        /// The dst process is the current process
        const DST_PROCESS_SELF = 1 << 7;
    }
}

bitflags! {
    pub struct CapDestroyFlags: u32 {
        /// Destroy the capability from the current process rather than the target process passed in
        const PROCESS_SELF = 1;
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
    /// Used to specify access permissions on memory mappings
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
    pub struct ThreadDestroyFlags: u32 {
        const DESTROY_OTHER = 1;
    }
}

bitflags! {
    /// The first three bits of flags are the same as MemoryMappingFlags, additonal options are here
    pub struct MemoryMapFlags: u32 {
        const MAX_SIZE = 1 << 3;
    }
}

bitflags! {
    pub struct MemoryUpdateMappingFlags: u32 {
        const UPDATE_SIZE = 1;
    }
}

bitflags! {
    /// Used by memory_resize syscall
    pub struct MemoryResizeFlags: u32 {
        const IN_PLACE = 1;
        const GROW_MAPPING = 1 << 1;
    }
}

bitflags! {
    /// Used by `chennel_sync_send` and `channel_sync_recv`
    pub struct ChannelSyncFlags: u32 {
        const TIMEOUT = 1;
    }
}