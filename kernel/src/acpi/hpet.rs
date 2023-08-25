use bytemuck::{Pod, Zeroable};

use super::{Sdt, SdtHeader};
use crate::prelude::*;

#[repr(C, packed)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct Hpet {
    header: SdtHeader,
}

impl Sdt for Hpet {
    fn header(&self) -> &SdtHeader {
        &self.header
    }
}

impl TrailerInit for Hpet {
    fn size(&self) -> usize {
        self.header.size()
    }
}