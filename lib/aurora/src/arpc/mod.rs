use serde::{Serialize, Deserialize};
use thiserror_no_std::Error;
use sys::Reply;
use futures::{select_biased, StreamExt};

use crate::{async_runtime::async_sys::{AsyncChannel, AsyncDropCheckReciever}, collections::MessageVec};

#[derive(Serialize, Deserialize)]
pub struct RpcCallData {
    pub service_id: u64,
    pub method_id: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Error)]
pub enum RpcError {
    #[error("Invalid rpc service id")]
    InvalidServiceId,
    #[error("Invalid rpc method id")]
    InvalidMethodId,
    #[error("Failed to deserialize rpc method arguments")]
    SerializationError,
}

pub fn respond_success<T: Serialize>(reply: Reply, data: T) {
    match aser::to_bytes_count_cap::<Result<T, RpcError>, MessageVec<u8>>(&Ok(data)) {
        // panic safety: response data should have non zero size
        Ok(data) => {
            // TODO: log error if error occurs
            let _ = reply.reply(&data.message_buffer().unwrap());
        },
        Err(_) => respond_error(reply, RpcError::SerializationError),
    }
}

pub fn respond_error(reply: Reply, error: RpcError) {
    let error: Result<(), RpcError> = Err(error);
    let response_data: MessageVec<u8> = aser::to_bytes(&error, 0)
        .expect("failed to serialize rpc error response");

    // panic safety: response data should have non zero size
    // TODO: log error if error occurs
    let _ = reply.reply(&response_data.message_buffer().unwrap());
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