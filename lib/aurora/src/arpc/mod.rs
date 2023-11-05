use sys::Reply;
use futures::{select_biased, StreamExt};

use crate::async_runtime::async_sys::{AsyncChannel, AsyncDropCheckReciever};

pub struct RpcCallData<'a> {
    service_id: u64,
    method_id: u32,
    data: &'a [u8],
}

pub trait RpcService {
    async fn call(&self, rpc_data: RpcCallData, reply: Reply);
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
            },
            _ = drop_future => break,
        }
    }
}