#![no_std]

extern crate std;

use arpc::ServerRpcEndpoint;
use aurora::env;
use sys::MmioAllocator;

fn main() {
    let args = env::args();

    let server_endpoint: ServerRpcEndpoint = args.named_arg("server_endpoint")
        .expect("provided hwaccess server endpoint is invalid");

    let mmio_allocator: MmioAllocator = args.named_arg("mmio_allocator")
        .expect("no mmio allocator provided to hwaccess server");
}