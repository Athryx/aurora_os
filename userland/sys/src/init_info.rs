use serde::{Serialize, Deserialize};

use crate::MmioAllocator;

/// A serialized version of this is passed into the startup data for the firt process
#[derive(Debug, Serialize, Deserialize)]
pub struct InitInfo {
    pub initrd_address: usize,
    pub mmio_allocator: MmioAllocator,
}