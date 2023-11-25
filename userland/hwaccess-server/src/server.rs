use aurora::prelude::*;
use aurora::service::{AppService, Service, NamedPermission};
use sys::{PhysMem, Key};

use crate::HwAccessServer;
use crate::pci::{PciDeviceInfo, Pci};

pub struct HwAccessServerImpl {
    pci_devices: Pci,
}

impl HwAccessServerImpl {
    pub fn new(pci_devices: Pci) -> Self {
        HwAccessServerImpl {
            pci_devices,
        }
    }
}

impl AppService for HwAccessServerImpl {
    fn get_permissions(&self) -> Vec<NamedPermission> {
        Vec::new()
    }

    fn new_session_permissions(&self, perms: Vec<Key>) -> Service {
        todo!()
    }
}

#[arpc::service_impl]
impl HwAccessServer for HwAccessServerImpl {
    fn get_pci_devices(&self) -> Vec<PciDeviceInfo> {
        let mut out = Vec::new();

        for device in self.pci_devices.devices().iter() {
            out.push(device.device_info())
        }

        out
    }

    fn get_pci_mem(&self, device: PciDeviceInfo) -> Option<PhysMem> {
        Some(self.pci_devices.get_device(device)?.get_phys_mem())
    }
}