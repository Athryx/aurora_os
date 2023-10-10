use futures::Future;
use sys::{DropCheckReciever, CapDrop, KResult};

use crate::generate_async_wrapper;

pub struct AsyncDropCheckReciever(DropCheckReciever);

impl AsyncDropCheckReciever {
    pub fn handle_drop<'a>(&'a self) -> impl Future<Output = KResult<usize>> + 'a {
        AsyncHandleDrop::Unpolled((&self.0,))
    }
}

impl From<DropCheckReciever> for AsyncDropCheckReciever {
    fn from(value: DropCheckReciever) -> Self {
        AsyncDropCheckReciever(value)
    }
}

generate_async_wrapper!(
    AsyncHandleDrop,
    (&'a DropCheckReciever,),
    usize,
    CapDrop,
    |reciever: (&DropCheckReciever,), event_pool, event_id| {
        reciever.0.handle_cap_drop_async(event_pool, event_id, true)
    },
    |event: CapDrop| event.data,
);