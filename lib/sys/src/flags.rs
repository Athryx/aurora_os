//! Flags use by some aurora kernel syscalls

use bitflags::bitflags;

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
    /// Used by `chennel_sync_send` and channel_sync_recv`
    pub struct ChannelSyncFlags: u32 {
        const TIMEOUT = 1;
    }
}