#![no_std]

extern crate std;

use aurora::env;
use arpc::{ServerRpcEndpoint, arpc_impl, run_rpc_service};
use std::prelude::*;

use fs_server::FsServer;

struct FsServerImpl;

#[arpc_impl]
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

    asynca::block_in_place(run_rpc_service(rpc_endpoint, FsServerImpl));
}
