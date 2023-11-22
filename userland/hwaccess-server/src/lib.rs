#![no_std]

#![feature(associated_type_defaults)]
#![feature(trait_alias)]
#![feature(decl_macro)]

use serde::{Serialize, Deserialize};
use sys::PhysMem;
use aurora::prelude::*;
use aurora::service::AppService;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PciDeviceInfo {
    pub segment_group: u16,
    pub bus_id: u8,
    pub slot_id: u8,
    pub function_id: u8,
    pub vendor_id: u16,
    pub device_id: u16,
    pub class: u8,
    pub subclass: u8,
    pub prog_if: u8,
}

// These are various classes and subclass numbers used by pci
pub const CLASS_MASS_STORAGE: u8 = 0x1;
pub const SUBCLASS_SERIAL_ATA: u8 = 0x6;
pub const PROG_IF_AHCI: u8 = 0x1;

// TODO: convert this to use vfs like service maybe when that is done
// this is kind of mvp service api right now just to get fs server working
#[arpc::service(service_id = 10, name = "HwAccess", AppService = aurora::service)]
pub trait HwAccessServer: AppService {
    fn get_pci_devices(&self) -> Vec<PciDeviceInfo>;

    fn get_pci_mem(&self, device: PciDeviceInfo) -> Option<PhysMem>;
}