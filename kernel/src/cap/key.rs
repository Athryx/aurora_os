use core::sync::atomic::{AtomicU64, Ordering};
use super::{CapObject, CapType};

static NEXT_KEY_ID: AtomicU64 = AtomicU64::new(0);

/// A capability which is globally unique identifier
/// 
/// It is often used to authenticate actions with other servers
#[derive(Debug, Clone, Copy)]
pub struct Key {
    id: u64,
}

impl Key {
    pub fn new() -> Self {
        Key {
            id: NEXT_KEY_ID.fetch_add(1, Ordering::Relaxed),
        }
    }

    pub fn id(&self) -> u64 {
        self.id
    }
}

impl CapObject for Key {
    const TYPE: CapType = CapType::Key;
}