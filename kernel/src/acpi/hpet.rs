use super::{Sdt, SdtHeader};
use crate::prelude::*;

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct Hpet {
    header: SdtHeader,
}

impl Sdt for Hpet {
    fn header(&self) -> &SdtHeader {
        &self.header
    }
}
