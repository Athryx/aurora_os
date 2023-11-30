pub mod config_space;

use serde::{Serialize, Deserialize};
use acpi::mcfg::Mcfg;
use bit_utils::Size;
use aurora::{this_context, addr_space, allocator::addr_space::{MapPhysMemArgs, RegionPadding}};
use aurora::prelude::*;
use sys::{PhysMem, MemoryMappingOptions, MemoryCacheSetting};

use crate::{AcpiTables, pmem_access};
use config_space::{PciConfigSpaceHeader, CONFIG_SPACE_SIZE, VENDOR_ID_INVALID};

pub const DEVICE_PER_BUS: usize = 32;
pub const FUNCTION_PER_DEVICE: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PciDeviceInfo {
    pub device_address: PciDeviceAddress,
    pub device_id: PciDeviceId,
    pub device_type: PciDeviceType,
}

/// Represents where on the pci bus this device is located
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PciDeviceAddress {
    pub segment_group: u16,
    pub bus_id: u8,
    pub slot_id: u8,
    pub function_id: u8,
}

/// Represents which particular model of device the pci device is
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PciDeviceId {
    pub vendor_id: u16,
    pub device_id: u16,
}

/// Represents which type of device this pci device is
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PciDeviceType {
    pub class: u8,
    pub subclass: u8,
    pub prog_if: u8,
}

// These are various classes and subclass numbers used by pci
pub const CLASS_MASS_STORAGE: u8 = 0x1;
pub const SUBCLASS_SERIAL_ATA: u8 = 0x6;
pub const PROG_IF_AHCI: u8 = 0x1;

pub struct PciDevice {
    device_address: PciDeviceAddress,
    device_id: PciDeviceId,
    device_type: PciDeviceType,
    mmio_phys_addr: usize,
    config_space: PciConfigSpaceHeader,
}

impl PciDevice {
    unsafe fn new(device_address: PciDeviceAddress, config_space: PciConfigSpaceHeader, mmio_phys_addr: usize) -> Option<Self> {
        let vendor_id = config_space.vendor_id();
        if vendor_id == VENDOR_ID_INVALID {
            None
        } else {
            let device_id = PciDeviceId {
                vendor_id,
                device_id: config_space.device_id(),
            };

            let device_type = PciDeviceType {
                class: config_space.class_code(),
                subclass: config_space.subclass(),
                prog_if: config_space.prog_if(),
            };

            Some(PciDevice {
                device_address,
                device_id,
                device_type,
                mmio_phys_addr,
                config_space,
            })
        }
    }

    pub fn device_address(&self) -> PciDeviceAddress {
        self.device_address
    }

    pub fn device_id(&self) -> PciDeviceId {
        self.device_id
    }

    pub fn device_type(&self) -> PciDeviceType {
        self.device_type
    }

    pub fn device_info(&self) -> PciDeviceInfo {
        PciDeviceInfo {
            device_address: self.device_address,
            device_id: self.device_id,
            device_type: self.device_type,
        }
    }

    pub fn get_phys_mem(&self) -> PhysMem {
        pmem_access().allocator
            .alloc(&this_context().allocator, self.mmio_phys_addr, Size::from_bytes(CONFIG_SPACE_SIZE))
            .expect("could not get phys mem for pci device")
    }
}

pub struct Pci {
    devices: Vec<PciDevice>,
}

impl Pci {
    pub fn new(acpi_tables: &AcpiTables) -> Self {
        let mcfg = acpi_tables.find_table::<Mcfg>()
            .expect("could not find mcfg table");

        let mut devices = Vec::new();
    
        for entry in mcfg.entries() {
            // map entry in memory
            let bus_count = entry.bus_number_end as usize - entry.bus_number_start as usize + 1;
            let entry_count = bus_count * DEVICE_PER_BUS * FUNCTION_PER_DEVICE;
            let entry_size = Size::from_bytes(CONFIG_SPACE_SIZE * entry_count);
    
            let phys_mem = pmem_access().allocator
                .alloc(&this_context().allocator, entry.base_address as usize, entry_size)
                .expect("could not get physmem for pci config spaces");
    
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
            }).expect("could not map physical memory for acpi config space");
    
            // TODO: figure out if bus_number_end is inclusive or exclusive
            for bus_id in entry.bus_number_start..=entry.bus_number_end {
                let bus_index = bus_id - entry.bus_number_start;
    
                for device_id in 0..DEVICE_PER_BUS {
                    for function in 0..FUNCTION_PER_DEVICE {
                        let index = bus_index as usize * (DEVICE_PER_BUS * FUNCTION_PER_DEVICE) + device_id * FUNCTION_PER_DEVICE + function;
                        let config_space_address = map_result.address + CONFIG_SPACE_SIZE * index;
    
                        let config_space = unsafe {
                            PciConfigSpaceHeader::from_addr(config_space_address)
                        };

                        let device_address = PciDeviceAddress {
                            segment_group: entry.pci_segment_group,
                            bus_id,
                            slot_id: device_id as u8,
                            function_id: function as u8,
                        };

                        let mmio_phys_addr = entry.base_address as usize + CONFIG_SPACE_SIZE * index;
                        let device = unsafe {
                            PciDevice::new(device_address, config_space, mmio_phys_addr)
                        };
    
                        if let Some(device) = device {
                            devices.push(device);
                        }
                    }
                }
            }
        }

        Pci {
            devices
        }
    }

    pub fn devices(&self) -> &[PciDevice] {
        &self.devices
    }

    pub fn get_device(&self, device_address: PciDeviceAddress) -> Option<&PciDevice> {
        for device in self.devices.iter() {
            if device.device_address() == device_address {
                return Some(device);
            }
        }

        None
    }
}