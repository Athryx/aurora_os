use aurora::prelude::*;
use hwaccess_server::{HwAccess, HwAccessAsync, PciDeviceInfo};

use super::{DiskAccess, DiskCompletion};

pub struct AhciBackend {

}

impl AhciBackend {
    pub async fn new(hwaccess: &HwAccess, device_info: PciDeviceInfo) -> Self {
        dprintln!("ahci device detected");

        let phys_mem = hwaccess.get_pci_mem(device_info).await
            .expect("could not get memory for pci config space of ahci controller");

        todo!()
    }
}

impl DiskAccess for AhciBackend {
    unsafe fn read_sectors(&self, sector_num: usize, sector_count: usize, dest_addr: usize) -> DiskCompletion {
        todo!()
    }

    unsafe fn write_sectors(&self, sector_num: usize, sector_count: usize, src_addr: usize) -> DiskCompletion {
        todo!()
    }
}