use serde::{Serialize, Deserialize};
use thiserror_no_std::Error;
use sys::{Reply, DropCheck, KResult, Channel, CapFlags, CspaceTarget, SysErr, cap_clone};
use futures::{select_biased, StreamExt};
pub use arpc_derive::{arpc_interface, arpc_impl};

use crate::{async_runtime::async_sys::{AsyncChannel, AsyncDropCheckReciever}, collections::MessageVec, this_context};

/// A version of `RpcCall` which doesn't contain the arguments
/// 
/// This is so we can check which method is called first,
/// and let that method deserialize the data it is expecting
#[derive(Serialize, Deserialize)]
pub struct RpcCallMethod {
    pub service_id: u64,
    pub method_id: u32,
}

#[derive(Serialize, Deserialize)]
pub struct RpcCall<T> {
    pub service_id: u64,
    pub method_id: u32,
    pub args: T,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Error)]
pub enum RpcError {
    #[error("Invalid rpc service id")]
    InvalidServiceId,
    #[error("Invalid rpc method id")]
    InvalidMethodId,
    #[error("Failed to deserialize rpc method arguments")]
    SerializationError,
    #[error("A system error occure: {0}")]
    SysErr(#[from] SysErr),
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

#[derive(Serialize, Deserialize)]
pub struct ClientRpcEndpoint {
    channel: AsyncChannel,
    drop_check: DropCheck,
}

impl ClientRpcEndpoint {
    pub async fn call<T: Serialize, U: for<'de> Deserialize<'de>>(&self, data: RpcCall<T>) -> Result<U, RpcError> {
        let serialized_data: MessageVec<u8> = aser::to_bytes_count_cap(&data)
            .or(Err(RpcError::SerializationError))?;

        // panic safety: the serialized data should have non zero length
        let response = self.channel.call(serialized_data.message_buffer().unwrap()).await?;

        let response = unsafe {
            // safety: this is called as soon as await resolves
            aser::from_bytes(response.as_slice())
                .or(Err(RpcError::SerializationError))?
        };

        response
    }
}

#[derive(Serialize, Deserialize)]
pub struct ServerRpcEndpoint {
    channel: AsyncChannel,
    drop_check_reciever: AsyncDropCheckReciever,
}

/// Creates a client and server endpoint for rpc
fn make_endpoints() -> KResult<(ClientRpcEndpoint, ServerRpcEndpoint)> {
    let server_channel = Channel::new(CapFlags::all(), &this_context().allocator)?;
    let client_channel = cap_clone(
        CspaceTarget::Current,
        CspaceTarget::Current,
        &server_channel,
        CapFlags::READ | CapFlags::PROD | CapFlags::UPGRADE,
    )?;

    let (drop_check, drop_check_reciever) = DropCheck::new(&this_context().allocator, 0)?;

    let client_endpoint = ClientRpcEndpoint {
        channel: client_channel.into(),
        drop_check,
    };

    let server_endpoint = ServerRpcEndpoint {
        channel: server_channel.into(),
        drop_check_reciever: drop_check_reciever.into(),
    };

    Ok((client_endpoint, server_endpoint))
}

pub async fn run_rpc_service<T: RpcService>(
    server_endpoint: ServerRpcEndpoint,
    service: T,
) {
    let mut message_stream = server_endpoint.channel.recv_repeat();
    let mut drop_future = server_endpoint.drop_check_reciever.handle_drop();

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