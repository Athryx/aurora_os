use sys::{CapType, EventData, CapDrop};

use crate::event::{BroadcastEventEmitter, BroadcastEventListener};
use crate::prelude::*;
use crate::container::Arc;
use crate::alloc::HeapRef;
use crate::sync::IMutex;

use super::CapObject;

#[derive(Debug)]
pub struct DropCheck {
    reciever: Arc<DropCheckReciever>,
}

impl Drop for DropCheck {
    fn drop(&mut self) {
        // no way to report error, just ignore
        let _ = self.reciever.notify_listeners();
    }
}

impl CapObject for DropCheck {
    const TYPE: CapType = CapType::DropCheck;
}

#[derive(Debug)]
pub struct DropCheckReciever {
    data: usize,
    drop_event: IMutex<BroadcastEventEmitter>,
}

impl DropCheckReciever {
    /// Notify listeners the drop check has been triggered
    pub fn notify_listeners(&self) -> KResult<()> {
        self.drop_event.lock().emit_event(EventData::CapDrop(CapDrop {
            data: self.data,
        }))
    }

    pub fn add_drop_event_listener(&self, listener: BroadcastEventListener) -> KResult<()> {
        self.drop_event.lock().add_listener(listener)
    }
}

impl CapObject for DropCheckReciever {
    const TYPE: CapType = CapType::DropCheckReciever;
}

/// Creates a drop check and a drop check reciever which is listening for that drop check to be dropped
pub fn drop_check_pair(data: usize, allocator: HeapRef) -> KResult<(Arc<DropCheck>, Arc<DropCheckReciever>)> {
    let reciever = Arc::new(DropCheckReciever {
        data,
        drop_event: IMutex::new(BroadcastEventEmitter::new(allocator.clone())),
    }, allocator.clone())?;

    let drop_check = Arc::new(DropCheck {
        reciever: reciever.clone(),
    }, allocator)?;

    Ok((drop_check, reciever))
}