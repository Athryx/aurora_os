//! Numbers used by all aurora kernel syscalls

pub const PRINT_DEBUG: u32 = 0;

pub const THREAD_GROUP_NEW: u32 = 1;
pub const THREAD_GROUP_EXIT: u32 = 2;
pub const THREAD_NEW: u32 = 3;
pub const THREAD_YIELD: u32 = 4;
pub const THREAD_DESTROY: u32 = 5;
pub const THREAD_SUSPEND: u32 = 6;
pub const THREAD_RESUME: u32 = 7;

pub const CAP_CLONE: u32 = 8;
pub const CAP_DESTROY: u32 = 9;

pub const MEMORY_MAP: u32 = 11;
pub const MEMORY_UNMAP: u32 = 12;
pub const MEMORY_UPDATE_MAPPING: u32 = 13;
pub const MEMORY_NEW: u32 = 14;
pub const MEMORY_GET_SIZE: u32 = 15;
pub const MEMORY_RESIZE: u32 = 16;

pub const CHANNEL_NEW: u32 = 27;
pub const CHANNEL_TRY_SEND: u32 = 28;
pub const CHANNEL_SYNC_SEND: u32 = 29;
pub const CHANNEL_ASYNC_SEND: u32 = 30;
pub const CHANNEL_TRY_RECV: u32 = 31;
pub const CHANNEL_SYNC_RECV: u32 = 32;
pub const CHANNEL_ASYNC_RECV: u32 = 33;

pub const KEY_NEW: u32 = 38;
pub const KEY_ID: u32 = 39;

pub const DROP_CHECK_NEW: u32 = 40;