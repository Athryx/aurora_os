use crate::sched::ThreadHandle;

mod broadcast_event_emitter;
mod event_pool;
mod queue_event_emitter;

#[derive(Debug)]
pub enum EventListenerRef {
    Thread(*const ThreadHandle),
}