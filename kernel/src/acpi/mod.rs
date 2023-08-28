use crate::prelude::*;

pub mod hpet;
pub mod madt;
pub mod rsdt;

use bytemuck::{Pod, bytes_of, Zeroable};
use hpet::Hpet;
use madt::Madt;
use rsdt::Rsdt;

/// Type of the acpi table
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SdtType {
    /// root system descriptor table
    Rsdt,
    /// extended system descriptor table (64 bit version of rsdp)
    Xsdt,
    /// multiple APIC description table
    Madt,
    /// High precision event timer table
    Hpet,
}

#[derive(Debug, Clone, Copy)]
pub enum AcpiTable<'a> {
    Rsdt(WithTrailer<'a, Rsdt>),
    Madt(WithTrailer<'a, Madt>),
    Hpet(WithTrailer<'a, Hpet>),
}

impl<'a> AcpiTable<'a> {
    pub fn assume_rsdt(&self) -> Option<WithTrailer<'a, Rsdt>> {
        if let Self::Rsdt(rsdt) = self {
            Some(*rsdt)
        } else {
            None
        }
    }

    pub fn assume_madt(&self) -> Option<WithTrailer<'a, Madt>> {
        if let Self::Madt(madt) = self {
            Some(*madt)
        } else {
            None
        }
    }

    pub fn assume_hpet(&self) -> Option<WithTrailer<'a, Hpet>> {
        if let Self::Hpet(hpet) = self {
            Some(*hpet)
        } else {
            None
        }
    }
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct SdtHeader {
    signature: [u8; 4],
    size: u32,
    revision: u8,
    checksum: u8,
    oemid: [u8; 6],
    oem_table_id: [u8; 8],
    oem_revision: u32,
    creator_id: u32,
    creator_revision: u32,
}

impl SdtHeader {
    pub fn size(&self) -> usize {
        self.size as usize
    }

    pub fn sdt_type(&self) -> Option<SdtType> {
        let s = &self.signature;
        // can't us match here
        Some(if s == "APIC".as_bytes() {
            SdtType::Madt
        } else if s == "RSDT".as_bytes() {
            SdtType::Rsdt
        } else if s == "XSDT".as_bytes() {
            SdtType::Xsdt
        } else if s == "HPET".as_bytes() {
            SdtType::Hpet
        } else {
            // TODO: add new acpi table types here
            return None;
        })
    }
}

pub trait Sdt: Pod {
    fn header(&self) -> &SdtHeader;

    fn sdt_type(&self) -> Option<SdtType> {
        self.header().sdt_type()
    }
}

impl<T: Sdt> WithTrailer<'_, T> {
    fn validate(&self) -> bool {
        let mut sum: usize = 0;

        for n in bytes_of(&self.data) {
            sum += *n as usize;
        }

        for n in self.trailer {
            sum += *n as usize;
        }

        sum % 0x100 == 0
    }
}