mod ahci;

use aurora::prelude::*;
use hwaccess_server::{HwAccess, HwAccessAsync};
use hwaccess_server::pci::{CLASS_MASS_STORAGE, SUBCLASS_SERIAL_ATA, PROG_IF_AHCI};

use crate::error::FsError;

trait DiskAccess {
    unsafe fn read_sectors(&self, sector_num: usize, sector_count: usize, dest_addr: usize) -> DiskCompletion;
    unsafe fn write_sectors(&self, sector_num: usize, sector_count: usize, src_addr: usize) -> DiskCompletion;
}

/// Signals when a disk read or write has completed
pub struct DiskCompletion {

}

/// Beckend to a disk which allows reading and writing to different sectors
pub struct FsBackend {
    disk_access: Box<dyn DiskAccess>,
}

impl FsBackend {
    fn new<T: DiskAccess + 'static>(disk_access: T) -> Self {
        FsBackend {
            disk_access: Box::new(disk_access),
        }
    }
}

/// Queries the hwaccess server for all disks and constructs an FsBackend for each one
pub async fn get_backends(hwaccess_server: HwAccess) -> Result<Vec<FsBackend>, FsError> {
    let mut backends = Vec::new();
    let pci_devices = hwaccess_server.get_pci_devices().await;

    for device in pci_devices.iter() {
        let device_type = device.device_type;
        if device_type.class == CLASS_MASS_STORAGE {
            if device_type.subclass == SUBCLASS_SERIAL_ATA && device_type.prog_if == PROG_IF_AHCI {
                backends.push(
                    FsBackend::new(ahci::AhciBackend::new(&hwaccess_server, *device).await?),
                );
            }
        }
    }

    Ok(backends)
}