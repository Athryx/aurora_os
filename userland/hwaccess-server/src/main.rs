#![no_std]

extern crate alloc;
extern crate std;

mod acpi_handler;

use acpi::AcpiTables;
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

    let acpi_tables = unsafe {
        acpi_handler::read_acpi_tables(mmio_allocator, rsdp)
    };
}