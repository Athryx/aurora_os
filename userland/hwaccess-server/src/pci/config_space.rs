use core::ptr::NonNull;

use volatile::{VolatilePtr, map_field};

pub const CONFIG_SPACE_SIZE: usize = 4096;

pub const VENDOR_ID_INVALID: u16 = 0xffff;

// FIXME: get this to be packed without causing compile error in map_field macro
#[repr(C)]
pub struct PciConfigSpaceHeader {
    pub vendor_id: u16,
    pub device_id: u16,
    pub command: u16,
    pub status: u16,
    pub revision_id: u8,
    pub prog_if: u8,
    pub subclass: u8,
    pub class_code: u8,
    pub cache_line_size: u8,
    pub latency_timer: u8,
    pub header_type: u8,
    pub bist: u8,
}

impl PciConfigSpaceHeader {
    /// Safety: address must be the address of valid pci config space
    pub unsafe fn from_addr(address: usize) -> VolatilePtr<'static, PciConfigSpaceHeader> {
        unsafe {
            VolatilePtr::new(NonNull::new(address as *mut PciConfigSpaceHeader).unwrap())
        }
    }

    /// Safety: this pci config header must point to valid pci config space
    pub unsafe fn data(this: VolatilePtr<PciConfigSpaceHeader>) -> Option<VolatilePtr<PciConfigSpaceData>> {
        // bit 7 indicates if multiple function device, ignore that bit
        let header_type = map_field!(this.header_type).read() & 0b01111111;

        match header_type {
            0 => {
                let data_ptr = unsafe { this.as_raw_ptr().as_ptr().add(1) };
                let data_ptr = NonNull::new(data_ptr as *mut PciConfigSpaceData).unwrap();
                Some(VolatilePtr::new(data_ptr))
            },
            // TODO: other header types
            _ => None,
        }
    }
}

/// Applies to header type 0 only
#[repr(C)]
pub struct PciConfigSpaceData {
    pub bar0: u32,
    pub bar1: u32,
    pub bar2: u32,
    pub bar3: u32,
    pub bar4: u32,
    pub bar5: u32,
    pub cardbus_cis_pointer: u32,
    pub subsystem_vendor_id: u16,
    pub subsystem_id: u16,
    pub expansion_rom_base_address: u32,
    pub capabilities_pointer: u8,
    _reserved0: u8,
    _reserved1: u16,
    _reserved2: u32,
    pub interrupt_line: u8,
    pub interrupt_pin: u8,
    pub min_grant: u8,
    pub max_latency: u8,
}