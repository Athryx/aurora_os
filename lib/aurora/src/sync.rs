//! Synchronization primitives for aurora userspace

// TODO: write the kernel lock implementation for futexes, for now just reexport spin locks
pub use spin::{
    Mutex,
    MutexGuard,
    RwLock,
    RwLockReadGuard,
    RwLockWriteGuard,
    RwLockUpgradableGuard,
    Once,
    Lazy
};