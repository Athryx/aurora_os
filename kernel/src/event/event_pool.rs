use sys::CapType;

use crate::cap::CapObject;
use super::UserspaceBuffer;

#[derive(Debug, Clone, Copy)]
pub struct EventRange {
    offset: usize,
    size: usize,
}

#[derive(Debug, Clone, Copy)]
pub struct EventSection {
    head: EventRange,
    // the list of events might not be contigous in the ring buffer
    tail: Option<EventRange>,
}

#[derive(Debug)]
pub struct BoundedEventPool {
    buffer: UserspaceBuffer,
}

impl CapObject for BoundedEventPool {
    const TYPE: CapType = CapType::BoundedEventPool;
}