use sys::CapType;

use crate::{cap::CapObject, sync::{IMutex, IMutexGuard}};

#[derive(Debug)]
pub struct MessageCapacityInner {
    /// If this is None, this message capacity is unlimited
    max_size: Option<usize>,
    current_size: usize,
}

#[derive(Debug)]
pub struct MessageCapacity {
    inner: IMutex<MessageCapacityInner>,
}

impl MessageCapacity {
    pub fn new(max_size: Option<usize>) -> Self {
        MessageCapacity {
            inner: IMutex::new(MessageCapacityInner {
                max_size,
                current_size: 0,
            }),
        }
    }

    pub fn inner(&self) -> IMutexGuard<MessageCapacityInner> {
        self.inner.lock()
    }
}

impl CapObject for MessageCapacity {
    const TYPE: CapType = CapType::MessageCapacity;
}