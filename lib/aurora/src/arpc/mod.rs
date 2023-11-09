use serde::{Serialize, Deserialize};
use sys::Reply;
use futures::{select_biased, StreamExt};

use crate::async_runtime::async_sys::{AsyncChannel, AsyncDropCheckReciever};

#[derive(Serialize, Deserialize)]
pub struct RpcCallData {
    pub service_id: u64,
    pub method_id: u32,
}

pub trait RpcService {
    fn call(&self, data: &[u8], reply: Reply);
}

pub async fn make_rpc_service<T: RpcService>(
    channel: AsyncChannel,
    drop_check_reciever: AsyncDropCheckReciever,
    service: T,
) {
    let mut message_stream = channel.recv_repeat();
    let mut drop_future = drop_check_reciever.handle_drop();

    loop {
        select_biased! {
            message = message_stream.next() => {
                let Some(mut message) = message else {
                    break;
                };

                // ignore messages which don't have a reply (only handle call, not send)
                let Some(reply) = message.reply.take() else {
                    continue;
                };

                // safety: the event pool should not yet have been invalidated since we just recived the event
                unsafe {
                    service.call(message.as_slice(), reply);
                }
            },
            _ = drop_future => break,
        }
    }
}