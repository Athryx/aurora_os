use aurora::{prelude::*, addr_space, allocator::addr_space::{MapPhysMemArgs, RegionPadding, MemoryMappingOptions}};
use hwaccess_server::{HwAccess, HwAccessAsync, PciDeviceInfo};

use crate::error::FsError;
use super::{DiskAccess, DiskCompletion};

pub struct AhciBackend {

}

impl AhciBackend {
    pub async fn new(hwaccess: &HwAccess, device_info: PciDeviceInfo) -> Result<Self, FsError> {
        dprintln!("ahci device detected");

        let phys_mem = hwaccess.get_pci_mem(device_info).await
            .ok_or(FsError::DeviceMapError)?;

        let map_result = addr_space().map_phys_mem(MapPhysMemArgs {
            phys_mem,
            options: MemoryMappingOptions {
                read: true,
                write: true,
                ..Default::default()
            },
            address: None,
            padding: RegionPadding::default(),
        })?;

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