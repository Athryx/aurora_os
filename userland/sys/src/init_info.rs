use serde::{Serialize, Deserialize};

/// A serialized version of this is passed into the startup data for the firt process
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct InitInfo {
    pub initrd_address: usize,
}