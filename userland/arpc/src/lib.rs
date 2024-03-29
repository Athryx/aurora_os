#![no_std]

use serde::{Serialize, Deserialize};
use thiserror_no_std::Error;
use sys::{Reply, DropCheck, KResult, Channel, CapFlags, CspaceTarget, SysErr, cap_clone};
use futures::{select_biased, StreamExt};
use aurora_core::{this_context, collections::MessageVec};
use asynca::async_sys::{AsyncChannel, AsyncDropCheckReciever};
pub use arpc_derive::{service, service_impl};
// reexport sys, aser, and asynca for arpc_derive macro so dependancy on sys is not required
pub use sys;
pub use aser;
pub use asynca;

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

#[derive(Debug, Clone, Serialize, Deserialize, Error)]
pub enum RpcError {
    #[error("Invalid rpc service id")]
    InvalidServiceId,
    #[error("Invalid rpc method id")]
    InvalidMethodId,
    #[error("Failed to deserialize rpc method arguments: {0}")]
    SerializationError(#[from] aser::AserError),
    #[error("A system error occured: {0}")]
    SysErr(#[from] SysErr),
}

pub fn respond_success<T: Serialize>(reply: Reply, data: T) {
    match aser::to_bytes_count_cap::<Result<T, RpcError>, MessageVec<u8>>(&Ok(data)) {
        // panic safety: response data should have non zero size
        Ok(data) => {
            // TODO: log error if error occurs
            let _ = reply.reply(&data.message_buffer().unwrap());
        },
        Err(error) => respond_error(reply, RpcError::SerializationError(error)),
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

pub trait RpcClient {
    fn from_endpoint(endpoint: ClientRpcEndpoint) -> Self;
}

pub trait RpcService {
    type Client: RpcClient;

    fn call(&self, data: &[u8], reply: Reply);
}

#[derive(Serialize, Deserialize)]
pub struct ClientRpcEndpoint {
    channel: AsyncChannel,
    drop_check: DropCheck,
}

impl ClientRpcEndpoint {
    pub async fn call<T: Serialize, U: for<'de> Deserialize<'de>>(&self, data: RpcCall<T>) -> Result<U, RpcError> {
        let serialized_data: MessageVec<u8> = aser::to_bytes_count_cap(&data)?;

        // panic safety: the serialized data should have non zero length
        let response = self.channel.call(serialized_data.message_buffer().unwrap()).await?;

        let response = unsafe {
            // safety: this is called as soon as await resolves
            aser::from_bytes(response.as_slice())?
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
pub fn make_endpoints() -> KResult<(ClientRpcEndpoint, ServerRpcEndpoint)> {
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

pub fn launch_service<T: RpcService + 'static>(service: T) -> KResult<T::Client> {
    let (client_endpoint, server_endpoint) = make_endpoints()?;

    let client = T::Client::from_endpoint(client_endpoint);

    asynca::spawn(run_rpc_service(server_endpoint, service));

    Ok(client)
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
            result = drop_future => {
                result.expect("could not listen for drop check reciever");
                break;
            },
        }
    }
}