#![no_std]

extern crate alloc;
extern crate std;

use arpc::ServerRpcEndpoint;
use aurora::env;
use sys::{MmioAllocator, Rsdp};

fn main() {
    let args = env::args();

    let server_endpoint: ServerRpcEndpoint = args.named_arg("server_endpoint")
        .expect("provided hwaccess server endpoint is invalid");

    let mmio_allocator: MmioAllocator = args.named_arg("mmio_allocator")
        .expect("no mmio allocator provided to hwaccess server");

    let rsdp: Rsdp = args.named_arg("rsdp")
        .expect("no rsdp provided to hwacces-server");

    hwaccess_server::run(mmio_allocator, rsdp, server_endpoint);
}