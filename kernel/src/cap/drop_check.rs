use sys::CapType;

use crate::prelude::*;
use crate::container::Arc;
use crate::alloc::HeapRef;

use super::CapObject;

#[derive(Debug)]
pub struct DropCheck {
    reciever: Arc<DropCheckReciever>,
}

impl Drop for DropCheck {
    fn drop(&mut self) {
        self.reciever.notify_listeners();
    }
}

impl CapObject for DropCheck {
    const TYPE: CapType = CapType::DropCheck;
}

#[derive(Debug)]
pub struct DropCheckReciever {
    data: usize,
}

impl DropCheckReciever {
    /// Notify listeners the drop check has been triggered
    pub fn notify_listeners(&self) {
        todo!();
    }
}

impl CapObject for DropCheckReciever {
    const TYPE: CapType = CapType::DropCheckReciever;
}

/// Creates a drop check and a drop check reciever which is listening for that drop check to be dropped
pub fn drop_check_pair(data: usize, allocator: HeapRef) -> KResult<(Arc<DropCheck>, Arc<DropCheckReciever>)> {
    let reciever = Arc::new(DropCheckReciever {
        data,
    }, allocator.clone())?;

    let drop_check = Arc::new(DropCheck {
        reciever: reciever.clone(),
    }, allocator)?;

    Ok((drop_check, reciever))
}