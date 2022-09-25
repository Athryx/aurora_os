use super::{Sdt, SdtHeader};
use crate::hwa_iter::{HwaIter, HwaTag};
use crate::prelude::*;

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct Madt {
    header: SdtHeader,
    pub lapic_addr: u32,
    pub lapic_flags: u32,
}

impl Madt {
    pub fn iter(&self) -> HwaIter<MadtTag> {
        unsafe { HwaIter::from_struct(self, self.header.size()) }
    }
}

impl Sdt for Madt {
    fn header(&self) -> &SdtHeader {
        &self.header
    }
}

#[derive(Debug, Clone, Copy)]
pub enum MadtElem<'a> {
    ProcLocalApic(&'a ProcLocalApic),
    IoApic(&'a IoApic),
    IoApicSrcOverride(&'a IoApicSrcOverride),
    IoApicNmi(&'a IoApicNmi),
    LocalApicNmi(&'a LocalApicNmi),
    LocalApicOverride(&'a LocalApicOverride),
    ProcLocalX2Apic(&'a ProcLocalX2Apic),
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct MadtTag {
    typ: u8,
    size: u8,
}

impl HwaTag for MadtTag {
    type Elem<'a> = MadtElem<'a>;

    fn size(&self) -> usize {
        self.size as usize
    }

    fn elem(&self) -> Self::Elem<'_> {
        unsafe {
            match self.typ {
                0 => MadtElem::ProcLocalApic(self.raw_data()),
                1 => MadtElem::IoApic(self.raw_data()),
                2 => MadtElem::IoApicSrcOverride(self.raw_data()),
                3 => MadtElem::IoApicNmi(self.raw_data()),
                4 => MadtElem::LocalApicNmi(self.raw_data()),
                5 => MadtElem::LocalApicOverride(self.raw_data()),
                9 => MadtElem::ProcLocalX2Apic(self.raw_data()),
                _ => panic!("invalid or unsupported madt type"),
            }
        }
    }
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct ProcLocalApic {
    pub proc_id: u8,
    pub apic_id: u8,
    pub flags: u32,
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct IoApic {
    pub ioapic_id: u8,
    reserved: u8,
    pub ioapic_addr: u32,
    pub global_sysint_base: u32,
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct IoApicSrcOverride {
    pub bus_src: u8,
    pub irq_src: u8,
    pub global_sysint: u32,
    pub flags: u16,
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct IoApicNmi {
    pub nmi_src: u8,
    reserved: u8,
    pub flags: u16,
    pub global_sysint: u32,
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct LocalApicNmi {
    pub proc_id: u8,
    pub flags: u16,
    pub lint: u8,
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct LocalApicOverride {
    reserved: u16,
    pub addr: u64,
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct ProcLocalX2Apic {
    reserved: u16,
    pub x2_apic_id: u32,
    pub flags: u32,
    pub acpi_id: u32,
}
