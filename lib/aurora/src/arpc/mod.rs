use sys::Reply;

pub struct RpcCallData<'a> {
    service_id: u64,
    method_id: u32,
    data: &'a [u8],
}

pub trait RpcService {
    async fn call(&self, rpc_data: RpcCallData, reply: Reply);
}