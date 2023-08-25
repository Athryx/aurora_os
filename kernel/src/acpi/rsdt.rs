use bytemuck::{Pod, Zeroable};

use crate::prelude::*;

use super::{SdtHeader, Sdt, SdtType, AcpiTable, madt::Madt, hpet::Hpet};

#[repr(transparent)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct Rsdt(SdtHeader);

impl Sdt for Rsdt {
    fn header(&self) -> &SdtHeader {
        &self.0
    }
}

impl TrailerInit for Rsdt {
    fn size(&self) -> usize {
        self.0.size()
    }
}

#[repr(transparent)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct TableAddress {
    address: u32,
}

impl WithTrailer<'_, Rsdt> {
    pub fn get_table(&self, table_type: SdtType) -> Option<AcpiTable> {
        for table in iter_unaligned_pod_data::<TableAddress>(self.trailer) {
            let address = phys_to_virt(table.address as usize);

            let sdt_header = unsafe { ptr::read_unaligned(address as *const SdtHeader) };

            if let Some(typ) = sdt_header.sdt_type() && typ == table_type {
                return match table_type {
                    SdtType::Madt => {
                        let madt = unsafe {
                            WithTrailer::from_pointer(address as *const Madt)
                        };

                        Some(AcpiTable::Madt(madt))
                    },
                    SdtType::Hpet => {
                        let hpet = unsafe {
                            WithTrailer::from_pointer(address as *const Hpet)
                        };

                        Some(AcpiTable::Hpet(hpet))
                    },
                    _ => None,
                };
            }
        }

        None
    }
}