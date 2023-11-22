#![no_std]

extern crate alloc;
extern crate std;

mod acpi_handler;
mod pci;
mod server;

use arpc::ServerRpcEndpoint;
use aurora::env;
use aurora::sync::Once;
use sys::{MmioAllocator, Rsdp};
use arpc::run_rpc_service;

use pci::Pci;
use server::HwAccessServerImpl;

pub type AcpiTables = acpi::AcpiTables<acpi_handler::AcpiHandlerImpl>;

static MMIO_ALLOCATOR: Once<MmioAllocator> = Once::new();

fn mmio_allocator() -> &'static MmioAllocator {
    MMIO_ALLOCATOR.get().unwrap()
}

fn main() {
    let args = env::args();

    let server_endpoint: ServerRpcEndpoint = args.named_arg("server_endpoint")
        .expect("provided hwaccess server endpoint is invalid");

    let mmio_allocator: MmioAllocator = args.named_arg("mmio_allocator")
        .expect("no mmio allocator provided to hwaccess server");

    let rsdp: Rsdp = args.named_arg("rsdp")
        .expect("no rsdp provided to hwacces-server");

    MMIO_ALLOCATOR.call_once(|| mmio_allocator);

    let acpi_tables = unsafe {
        acpi_handler::read_acpi_tables(rsdp)
    };

    let pci = Pci::new(&acpi_tables);
    let server = HwAccessServerImpl::new(pci);

    asynca::block_in_place(run_rpc_service(server_endpoint, server));
}