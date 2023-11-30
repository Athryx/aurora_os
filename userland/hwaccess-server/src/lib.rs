#![no_std]

#![feature(associated_type_defaults)]
#![feature(trait_alias)]
#![feature(decl_macro)]

mod acpi_handler;
mod error;
pub mod pci;
mod pmem_access;
mod server;

use pmem_access::PmemAccess;
use sys::PhysMem;
use aurora::prelude::*;
use aurora::service::AppService;
use arpc::ServerRpcEndpoint;
use aurora::sync::Once;
use sys::{MmioAllocator, Rsdp};
use arpc::run_rpc_service;

use pci::{Pci, PciDeviceAddress, PciDeviceInfo};
use server::HwAccessServerImpl;

type AcpiTables = acpi::AcpiTables<acpi_handler::AcpiHandlerImpl>;

// TODO: convert this to use vfs like service maybe when that is done
// this is kind of mvp service api right now just to get fs server working
#[arpc::service(service_id = 10, name = "HwAccess", AppService = aurora::service)]
pub trait HwAccessServer: AppService {
    fn get_pci_devices(&self) -> Vec<PciDeviceInfo>;

    fn get_pci_mem(&self, device: PciDeviceAddress) -> Option<PhysMem>;
}

static PMEM_ACCESS: Once<PmemAccess> = Once::new();

pub fn pmem_access() -> &'static PmemAccess {
    PMEM_ACCESS.get().unwrap()
}

pub fn run(mmio_allocator: MmioAllocator, rsdp: Rsdp, server_endpoint: ServerRpcEndpoint) {
    PMEM_ACCESS.call_once(|| mmio_allocator.into());

    let acpi_tables = unsafe {
        acpi_handler::read_acpi_tables(rsdp)
    };

    let pci = Pci::new(&acpi_tables);
    let server = HwAccessServerImpl::new(pci);

    asynca::block_in_place(run_rpc_service(server_endpoint, server));
}