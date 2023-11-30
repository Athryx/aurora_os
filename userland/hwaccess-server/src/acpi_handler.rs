use core::{mem::size_of, ptr::NonNull};

use aurora::addr_space;
use bit_utils::{align_down, PAGE_SIZE, Size};
use acpi::{AcpiHandler, PhysicalMapping, AcpiTables, rsdp::Rsdp};

use crate::pmem_access;

#[derive(Clone)]
pub struct AcpiHandlerImpl;

impl AcpiHandler for AcpiHandlerImpl {
    unsafe fn map_physical_region<T>(
        &self,
        physical_address: usize,
        size: usize,
    ) -> PhysicalMapping<Self, T> {
        assert!(size >= size_of::<T>());

        let pmem_data = pmem_access()
            .map_address_raw(physical_address, Size::from_bytes(size))
            .expect("acpi handler: could not map physical memory");

        let ptr = (pmem_data.base_virt_address + pmem_data.data_offset) as *mut T;

        unsafe {
            PhysicalMapping::new(
                physical_address,
                NonNull::new(ptr).unwrap(),
                size,
                pmem_data.size.bytes(),
                self.clone(),
            )
        }
    }

    fn unmap_physical_region<T>(region: &PhysicalMapping<Self, T>) {
        if region.mapped_length() == 0 {
            // this is dummy mapping for rsdp
            return;
        }

        let unmap_address = align_down(region.virtual_start().as_ptr() as usize, PAGE_SIZE);

        unsafe {
            addr_space().unmap_memory(unmap_address)
                .expect("acpi handler: failed to unmap physical region");
        }
    }
}

/// # Safety
/// 
/// Must pass in a valid rsdp
pub unsafe fn read_acpi_tables(mut rsdp: sys::Rsdp) -> AcpiTables<AcpiHandlerImpl> {
    let rsdp_ptr = NonNull::new(&mut rsdp as *mut sys::Rsdp as *mut Rsdp).unwrap();
    let rsdp_mapping = unsafe {
        PhysicalMapping::new(
            0,
            rsdp_ptr,
            // FIXME: this region length is acutally wrong and unsafe,
            // rsdp_ptr only points to v1 rsdp, but acpi rsdp could be v2
            // in practice no bad reads should occurr though
            size_of::<Rsdp>(),
            0,
            AcpiHandlerImpl,
        )
    };

    unsafe {
        AcpiTables::from_validated_rsdp(AcpiHandlerImpl, rsdp_mapping)
            .expect("failed to read acpi tables")
    }
}