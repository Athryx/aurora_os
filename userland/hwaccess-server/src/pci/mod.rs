mod config_space;

use core::ptr::NonNull;

use acpi::mcfg::Mcfg;
use bit_utils::Size;
use aurora::{this_context, addr_space, allocator::addr_space::{MapPhysMemArgs, RegionPadding}};
use sys::MemoryMappingFlags;
use volatile::{VolatilePtr, map_field};

use crate::{AcpiTables, mmio_allocator, pci::config_space::CONFIG_SPACE_SIZE};
use config_space::PciConfigSpace;

use self::config_space::VENDOR_ID_INVALID;

pub const DEVICE_PER_BUS: usize = 32;
pub const FUNCTION_PER_DEVICE: usize = 8;

struct PciDevice {
    segment_group: u16,
    bus_id: u8,
    device_id: u8,
    function_id: u8,
    config_space: VolatilePtr<'static, PciConfigSpace>,
}

impl PciDevice {
    unsafe fn new(segment_group: u16, bus_id: u8, device_id: u8, function_id: u8, data: NonNull<PciConfigSpace>) -> Option<Self> {
        let config_space = unsafe { VolatilePtr::new(data) };

        let vendor_id = map_field!(config_space.vendor_id).read();
        if vendor_id == VENDOR_ID_INVALID {
            None
        } else {
            Some(PciDevice {
                segment_group,
                bus_id,
                device_id,
                function_id,
                config_space,
            })
        }
    }
}

pub fn init(acpi_tables: &AcpiTables) {
    let mcfg = acpi_tables.find_table::<Mcfg>()
        .expect("could not find mcfg table");

    for entry in mcfg.entries() {
        // map entry in memory
        let bus_count = entry.bus_number_end as usize - entry.bus_number_start as usize + 1;
        let entry_count = bus_count * DEVICE_PER_BUS * FUNCTION_PER_DEVICE;
        let entry_size = Size::from_bytes(CONFIG_SPACE_SIZE * entry_count);

        let phys_mem = mmio_allocator().alloc(&this_context().allocator, entry.base_address as usize, entry_size)
            .expect("could not get physmem for pci config spaces");

        let map_result = addr_space().map_phys_mem(MapPhysMemArgs {
            phys_mem,
            flags: MemoryMappingFlags::READ | MemoryMappingFlags::WRITE,
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

                    let config_space_ptr = NonNull::new(config_space_address as *mut PciConfigSpace).unwrap();
                    let device = unsafe {
                        PciDevice::new(entry.pci_segment_group, bus_id, device_id as u8, function as u8, config_space_ptr)
                    };

                    if let Some(device) = device {
                        sys::dprintln!("device detected");
                        sys::dprintln!("bus id: {bus_id}");
                        sys::dprintln!("device id: {device_id}");
                        sys::dprintln!("function: {function}");
                    }
                }
            }
        }
        sys::dprintln!("{entry:?}");
    }
}