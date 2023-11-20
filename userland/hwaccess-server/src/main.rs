#![no_std]

extern crate alloc;
extern crate std;

mod acpi_handler;
mod pci;

use arpc::ServerRpcEndpoint;
use aurora::env;
use aurora::sync::Once;
use sys::{MmioAllocator, Rsdp};

use pci::Pci;

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

    acpi_tables.find_table::<acpi::madt::Madt>()
        .expect("could not find madt table");

    //acpi_tables.find_table::<acpi::bgrt::Bgrt>().expect("could not find bgrt table");

    acpi_tables.find_table::<acpi::fadt::Fadt>()
        .expect("could not find fadt table");

    acpi_tables.find_table::<acpi::hpet::HpetTable>()
        .expect("could not find hpet table");

    let pci = Pci::new(&acpi_tables);
}