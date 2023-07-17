use crate::{prelude::*, alloc::HeapRef};
use super::{EventListenerRef, EventPoolListenerRef, UserspaceBuffer};

#[derive(Debug)]
pub struct BroadcastEventEmitter {
    oneshot_listeners: Vec<EventListenerRef>,
    continous_listeners: Vec<EventPoolListenerRef>,
}

impl BroadcastEventEmitter {
    pub fn new(heap: HeapRef) -> Self {
        BroadcastEventEmitter {
            oneshot_listeners: Vec::new(heap.clone()),
            continous_listeners: Vec::new(heap),
        }
    }
}