#![no_std]

extern crate std;

mod disk_access;

use aurora::env;
use arpc::{ServerRpcEndpoint, run_rpc_service};
use hwaccess_server::HwAccess;
use std::prelude::*;

use fs_server::FsServer;

struct FsServerImpl;

#[arpc::service_impl]
impl FsServer for FsServerImpl {
    fn add(&self, a: usize, b: usize) -> usize {
        a + b
    }
}

fn main() {
    dprintln!("hello fs");

    let args = env::args();
    let rpc_endpoint: ServerRpcEndpoint = args.named_arg("server_endpoint")
        .expect("provided fs server rpc endpoint is invalid");

    let hwaccess: HwAccess = args.named_arg("hwaccess_server")
        .expect("no hwaccess_server endpoint provided");

    asynca::block_in_place(async move {
        let backends = disk_access::get_backends(hwaccess).await;
    });

    //asynca::block_in_place(run_rpc_service(rpc_endpoint, FsServerImpl));
}
