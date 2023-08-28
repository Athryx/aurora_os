use bytemuck::{Pod, Zeroable};

use super::{Sdt, SdtHeader};
use crate::prelude::*;
use crate::util::{HwaIter, HwaTag};

#[repr(C, packed)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct Madt {
    header: SdtHeader,
    pub lapic_addr: u32,
    pub lapic_flags: u32,
}

impl Sdt for Madt {
    fn header(&self) -> &SdtHeader {
        &self.header
    }
}

impl TrailerInit for Madt {
    fn size(&self) -> usize {
        self.header.size()
    }
}

impl<'a> WithTrailer<'a, Madt> {
    pub fn iter(&self) -> HwaIter<'a, MadtTag> {
        HwaIter::from(self.trailer)
    }
}

#[derive(Debug, Clone, Copy)]
pub enum MadtElem {
    ProcLocalApic(ProcLocalApic),
    IoApic(IoApic),
    IoApicSrcOverride(IoApicSrcOverride),
    IoApicNmi(IoApicNmi),
    LocalApicNmi(LocalApicNmi),
    LocalApicOverride(LocalApicOverride),
    ProcLocalX2Apic(ProcLocalX2Apic),
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct MadtTag {
    typ: u8,
    size: u8,
}

impl HwaTag for MadtTag {
    type Elem<'a> = MadtElem;

    fn size(&self) -> usize {
        self.size as usize
    }

    fn elem(this: WithTrailer<'_, Self>) -> Self::Elem<'_> {
        match this.data.typ {
            0 => MadtElem::ProcLocalApic(Self::data(&this)),
            1 => MadtElem::IoApic(Self::data(&this)),
            2 => MadtElem::IoApicSrcOverride(Self::data(&this)),
            3 => MadtElem::IoApicNmi(Self::data(&this)),
            4 => MadtElem::LocalApicNmi(Self::data(&this)),
            5 => MadtElem::LocalApicOverride(Self::data(&this)),
            9 => MadtElem::ProcLocalX2Apic(Self::data(&this)),
            _ => panic!("invalid or unsupported madt type"),
        }
    }
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct ProcLocalApic {
    pub proc_id: u8,
    pub apic_id: u8,
    pub flags: u32,
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct IoApic {
    pub ioapic_id: u8,
    reserved: u8,
    pub ioapic_addr: u32,
    pub global_sysint_base: u32,
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct IoApicSrcOverride {
    pub bus_src: u8,
    pub irq_src: u8,
    pub global_sysint: u32,
    pub flags: u16,
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct IoApicNmi {
    pub nmi_src: u8,
    reserved: u8,
    pub flags: u16,
    pub global_sysint: u32,
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct LocalApicNmi {
    pub proc_id: u8,
    pub flags: u16,
    pub lint: u8,
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct LocalApicOverride {
    reserved: u16,
    pub addr: u64,
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct ProcLocalX2Apic {
    reserved: u16,
    pub x2_apic_id: u32,
    pub flags: u32,
    pub acpi_id: u32,
}
