use bytemuck::{Pod, Zeroable, bytes_of};
use serde::{Serialize, Deserialize};

use crate::MmioAllocator;

#[repr(C, packed)]
#[derive(Debug, Clone, Copy, Pod, Zeroable, Serialize, Deserialize)]
pub struct Rsdp {
    pub signature: [u8; 8],
    pub checksum: u8,
    pub oemid: [u8; 6],
    pub revision: u8,
    pub rsdt_addr: u32,
}

impl Rsdp {
    // add up every byte and make sure lowest byte is equal to 0
    pub fn validate(&self) -> bool {
        let mut sum: usize = 0;
        let slice = bytes_of(self);

        for n in slice {
            sum += *n as usize;
        }

        sum % 0x100 == 0
    }
}

/// A serialized version of this is passed into the startup data for the firt process
#[derive(Debug, Serialize, Deserialize)]
pub struct InitInfo {
    pub initrd_address: usize,
    pub mmio_allocator: MmioAllocator,
    /// Copy of acpi root system descriptor pointer
    pub rsdp: Rsdp,
}