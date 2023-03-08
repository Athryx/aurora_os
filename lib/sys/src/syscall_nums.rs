//! Numbers used by all aurora kernel syscalls

pub const PRINT_DEBUG: u32 = 0;

pub const PROCESS_NEW: u32 = 1;
pub const PROCESS_EXIT: u32 = 2;
pub const THREAD_NEW: u32 = 3;
pub const THREAD_YIELD: u32 = 4;
pub const THREAD_SUSPEND: u32 = 6;

pub const MEMORY_MAP: u32 = 11;
pub const MEMORY_UNMAP: u32 = 12;
pub const MEMORY_NEW: u32 = 14;

pub const KEY_NEW: u32 = 38;
pub const KEY_ID: u32 = 39;

pub const SPAWNER_NEW: u32 = 46;
pub const SPAWNER_KILL_ALL: u32 = 47;