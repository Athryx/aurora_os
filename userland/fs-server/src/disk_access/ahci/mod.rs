use aurora::{prelude::*, addr_space, allocator::addr_space::{MapPhysMemArgs, RegionPadding, MemoryMappingOptions, MemoryCacheSetting}};
use volatile::map_field;
use hwaccess_server::{HwAccess, HwAccessAsync};
use hwaccess_server::pci::{PciDeviceInfo, config_space::PciConfigSpaceHeader};

use crate::error::FsError;
use super::{DiskAccess, DiskCompletion};

pub struct AhciBackend {

}

impl AhciBackend {
    pub async fn new(hwaccess: &HwAccess, device_info: PciDeviceInfo) -> Result<Self, FsError> {
        dprintln!("ahci device detected");

        let phys_mem = hwaccess.get_pci_mem(device_info.device_address).await
            .ok_or(FsError::DeviceMapError)?;

        let map_result = addr_space().map_phys_mem(MapPhysMemArgs {
            phys_mem,
            options: MemoryMappingOptions {
                read: true,
                write: true,
                cacheing: MemoryCacheSetting::Uncached,
                ..Default::default()
            },
            address: None,
            padding: RegionPadding::default(),
        })?;

        let config_space = unsafe {
            PciConfigSpaceHeader::from_addr(map_result.address)
        };

        // panic safety: this will not fail because ahci device always has type 0 header
        let config_data = config_space.data().unwrap();

        // TODO: make sure the controller is in ahci mode (osdev wiki says it can also be in ide mode)

        let ahci_mem_phys_addr = map_field!(config_data.bar5).read();
        dprintln!("{:x?}", ahci_mem_phys_addr);

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