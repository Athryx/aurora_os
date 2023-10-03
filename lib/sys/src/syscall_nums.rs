//! Numbers used by all aurora kernel syscalls

pub const PRINT_DEBUG: u32 = 0;

pub const THREAD_GROUP_NEW: u32 = 1;
pub const THREAD_GROUP_EXIT: u32 = 2;
pub const THREAD_NEW: u32 = 3;
pub const THREAD_YIELD: u32 = 4;
pub const THREAD_DESTROY: u32 = 5;
pub const THREAD_SUSPEND: u32 = 6;
pub const THREAD_RESUME: u32 = 7;
pub const THREAD_SET_PROPERTY: u32 = 8;
pub const THREAD_HANDLE_THREAD_EXIT_SYNC: u32 = 9;
pub const THREAD_HANDLE_THREAD_EXIT_ASYNC: u32 = 10;

pub const CAP_CLONE: u32 = 11;
pub const CAP_DESTROY: u32 = 12;

pub const ADDRESS_SPACE_NEW: u32 = 13;
pub const MEMORY_MAP: u32 = 14;
pub const MEMORY_UNMAP: u32 = 15;
pub const MEMORY_UPDATE_MAPPING: u32 = 16;
pub const MEMORY_NEW: u32 = 17;
pub const MEMORY_GET_SIZE: u32 = 18;
pub const MEMORY_RESIZE: u32 = 19;

pub const EVENT_POOL_NEW: u32 = 24;
pub const EVENT_POOL_MAP: u32 = 25;
pub const EVENT_POOL_AWAIT: u32 = 26;

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

pub fn syscall_name(syscall_num: u32) -> &'static str {
    match syscall_num {
        PRINT_DEBUG => "print_debug",
        THREAD_GROUP_NEW => "thread_group_new",
        THREAD_GROUP_EXIT => "thread_group_exit",
        THREAD_NEW => "thread_new",
        THREAD_YIELD => "thread_yield",
        THREAD_DESTROY => "thread_destroy",
        THREAD_SUSPEND => "thread_suspend",
        THREAD_RESUME => "thread_resume",
        THREAD_SET_PROPERTY => "thread_set_property",
        THREAD_HANDLE_THREAD_EXIT_SYNC => "thread_handel_thread_exit_sync",
        THREAD_HANDLE_THREAD_EXIT_ASYNC => "thread_handel_thread_exit_async",
        CAP_CLONE => "cap_clone",
        CAP_DESTROY => "cap_destroy",
        ADDRESS_SPACE_NEW => "address_space_new",
        MEMORY_MAP => "memory_map",
        MEMORY_UNMAP => "memory_unmap",
        MEMORY_UPDATE_MAPPING => "memory_update_mapping",
        MEMORY_NEW => "memory_new",
        MEMORY_GET_SIZE => "memory_get_size",
        MEMORY_RESIZE => "memory_resize",
        EVENT_POOL_NEW => "event_pool_new",
        EVENT_POOL_MAP => "event_pool_map",
        EVENT_POOL_AWAIT => "event_pool_await",
        CHANNEL_NEW => "channel_new",
        CHANNEL_TRY_SEND => "channel_try_send",
        CHANNEL_SYNC_SEND => "channel_sync_send",
        CHANNEL_ASYNC_SEND => "channel_async_send",
        CHANNEL_TRY_RECV => "channel_try_recv",
        CHANNEL_SYNC_RECV => "channel_sync_recv",
        CHANNEL_ASYNC_RECV => "channel_async_recv",
        KEY_NEW => "key_new",
        KEY_ID => "key_id",
        DROP_CHECK_NEW => "drop_check_new",
        _ => "invalid syscall",
    }
}