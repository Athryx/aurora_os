use sys::EventData;

use crate::{prelude::*, sched::WakeReason};
use crate::alloc::HeapRef;
use crate::sched::ThreadRef;
use super::EventPoolListenerRef;

#[derive(Debug)]
pub struct BroadcastEventEmitter {
    oneshot_listeners: Vec<BroadcastEventListener>,
    continous_listeners: Vec<EventPoolListenerRef>,
}

impl BroadcastEventEmitter {
    pub fn new(heap: HeapRef) -> Self {
        eprintln!("inside1");
        let out = BroadcastEventEmitter {
            oneshot_listeners: Vec::new(heap.clone()),
            continous_listeners: Vec::new(heap),
        };
        eprintln!("inside2");
        out
    }

    pub fn emit_event(&mut self, event_data: EventData) -> KResult<()> {
        while let Some(listener) = self.oneshot_listeners.pop() {
            listener.write_event(event_data)?;
        }

        for listener in self.continous_listeners.iter() {
            listener.write_event(event_data)?;
        }

        Ok(())
    }

    pub fn add_listener(&mut self, listener: BroadcastEventListener) -> KResult<()> {
        match listener {
            BroadcastEventListener::EventPool { auto_reque: true, event_pool } =>
                self.continous_listeners.push(event_pool),
            _ => self.oneshot_listeners.push(listener),
        }
    }
}

#[derive(Debug)]
pub enum BroadcastEventListener {
    Thread(ThreadRef),
    EventPool {
        event_pool: EventPoolListenerRef,
        auto_reque: bool,
    },
}

impl BroadcastEventListener {
    fn write_event(&self, event_data: EventData) -> KResult<()> {
        match self {
            Self::Thread(thread_ref) => {
                thread_ref.move_to_ready_list(WakeReason::EventRecieved(event_data));

                Ok(())
            },
            Self::EventPool { event_pool, .. } => {
                event_pool.write_event(event_data)?;

                Ok(())
            },
        }
    }
}