use core::ptr::NonNull;

use volatile::{VolatilePtr, map_field};

pub const CONFIG_SPACE_SIZE: usize = 4096;

pub const VENDOR_ID_INVALID: u16 = 0xffff;

pub const STATUS_HAS_CAPABILITIES: u16 = 1 << 4;

// FIXME: get this to be packed without causing compile error in map_field macro
#[repr(C)]
struct PciConfigSpaceHeaderRaw {
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

pub struct PciConfigSpaceHeader(VolatilePtr<'static, PciConfigSpaceHeaderRaw>);

impl PciConfigSpaceHeader {
    /// Safety: address must be the address of valid pci config space
    pub unsafe fn from_addr(address: usize) -> Self {
        let ptr = unsafe {
            VolatilePtr::new(NonNull::new(address as *mut PciConfigSpaceHeaderRaw).unwrap())
        };

        PciConfigSpaceHeader(ptr)
    }

    /// Returns the virtual address at which this pci config space is mapped
    pub fn virtual_address(&self) -> usize {
        self.0.as_raw_ptr().as_ptr() as usize
    }

    pub fn vendor_id(&self) -> u16 {
        let ptr = self.0;
        map_field!(ptr.vendor_id).read()
    }

    pub fn device_id(&self) -> u16 {
        let ptr = self.0;
        map_field!(ptr.device_id).read()
    }

    pub fn status(&self) -> u16 {
        let ptr = self.0;
        map_field!(ptr.status).read()
    }

    pub fn class_code(&self) -> u8 {
        let ptr = self.0;
        map_field!(ptr.class_code).read()
    }

    pub fn subclass(&self) -> u8 {
        let ptr = self.0;
        map_field!(ptr.subclass).read()
    }

    pub fn prog_if(&self) -> u8 {
        let ptr = self.0;
        map_field!(ptr.prog_if).read()
    }

    pub fn has_capabilities(&self) -> bool {
        self.status() & STATUS_HAS_CAPABILITIES != 0
    }

    pub fn capabilities(&self) -> Option<PciCapability> {
        if !self.has_capabilities() {
            return None;
        }

        let data = self.data()?;
        // lowest 2 bits of the capability pointer are reserved and should be masked off
        let capability_offset = map_field!(data.capabilities_pointer).read() & 0xfc;
        let capability_address = self.virtual_address() + capability_offset as usize;

        let ptr = unsafe {
            VolatilePtr::new(
                NonNull::new(capability_address as *mut PciCapabilityRaw).unwrap(),
            )
        };

        Some(PciCapability {
            config_space_header: self,
            capability_header: ptr,
        })
    }

    pub fn data(&self) -> Option<VolatilePtr<PciConfigSpaceData>> {
        let ptr = self.0;
        // bit 7 indicates if multiple function device, ignore that bit
        let header_type = map_field!(ptr.header_type).read() & 0b01111111;

        match header_type {
            0 => {
                let data_ptr = unsafe { ptr.as_raw_ptr().as_ptr().add(1) };
                let data_ptr = NonNull::new(data_ptr as *mut PciConfigSpaceData).unwrap();
                unsafe {
                    Some(VolatilePtr::new(data_ptr))
                }
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

pub struct PciCapability<'a> {
    config_space_header: &'a PciConfigSpaceHeader,
    capability_header: VolatilePtr<'a, PciCapabilityRaw>,
}

impl<'a> PciCapability<'a> {
    pub fn next_capability(&self) -> Option<PciCapability<'a>> {
        let ptr = self.capability_header;
        // this field does not have the 2 lowest bits reserved im pretty sure
        let next_capability = map_field!(ptr.next_capability).read();
        if next_capability == 0 {
            return None;
        }

        sys::dprintln!("next cap: {next_capability}");

        let capability_address = self.config_space_header.virtual_address() + next_capability as usize;

        let ptr = unsafe {
            VolatilePtr::new(
                NonNull::new(capability_address as *mut PciCapabilityRaw).unwrap(),
            )
        };

        Some(PciCapability {
            config_space_header: self.config_space_header,
            capability_header: ptr,
        })
    }

    pub fn capability_id(&self) -> u8 {
        let ptr = self.capability_header;
        map_field!(ptr.capability_id).read()
    }
}

/// Header for a pci capability
#[repr(C)]
pub struct PciCapabilityRaw {
    capability_id: u8,
    next_capability: u8,
}