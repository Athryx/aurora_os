#![no_std]

extern crate std;

use aurora::env;
use aurora::arpc::ServerRpcEndpoint;
use std::prelude::*;

async fn sum(a: u32) -> u32 {
    a + 97
}

async fn test() {
    dprintln!("sum {}", sum(32).await);
}

fn main() {
    dprintln!("hello fs");

    let args = env::args();
    let rpc_endpoint: ServerRpcEndpoint = args.named_arg("server_endpoint")
        .expect("provided fs server rpc endpoint is invalid");

    aurora::async_runtime::block_in_place(async {
        test().await;
    });
}
