use core::{mem::size_of, ptr::NonNull};
use alloc::rc::Rc;

use aurora::{this_context, addr_space, allocator::addr_space::{MapPhysMemArgs, RegionPadding}};
use bit_utils::{align_up, align_down, PAGE_SIZE, Size};
use sys::{MmioAllocator, MemoryMappingFlags};
use acpi::{AcpiHandler, PhysicalMapping, AcpiTables, rsdp::Rsdp};

#[derive(Clone)]
pub struct AcpiHandlerImpl(Rc<MmioAllocator>);

impl AcpiHandlerImpl {
    fn new(mmio: MmioAllocator) -> Self {
        AcpiHandlerImpl(Rc::new(mmio))
    }
}

impl AcpiHandler for AcpiHandlerImpl {
    unsafe fn map_physical_region<T>(
        &self,
        physical_address: usize,
        size: usize,
    ) -> PhysicalMapping<Self, T> {
        assert!(size >= size_of::<T>());

        let end_address = physical_address + size;

        let region_start_addr = align_down(physical_address, PAGE_SIZE);
        let region_end_addr = align_up(end_address, PAGE_SIZE);
        let region_size = Size::from_bytes(region_end_addr - region_start_addr);

        let phys_mem = self.0.alloc(&this_context().allocator, region_start_addr, region_size)
            .expect("acpi handler: failed to alloc physical memory region");

        let map_result = addr_space().map_phys_mem(MapPhysMemArgs {
            phys_mem,
            flags: MemoryMappingFlags::READ | MemoryMappingFlags::WRITE,
            address: None,
            padding: RegionPadding::default(),
        }).expect("acpi handler: failed to map physical memory");

        // offset from start of physical region we mapped to the actual requested data
        let data_offset = physical_address - region_start_addr;
        let data = (map_result.address + data_offset) as *mut T;
        let data = NonNull::new(data).unwrap();

        unsafe {
            PhysicalMapping::new(
                physical_address,
                data,
                size,
                map_result.size.bytes(),
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
pub unsafe fn read_acpi_tables(mmio_allocator: MmioAllocator, mut rsdp: sys::Rsdp) -> AcpiTables<AcpiHandlerImpl> {
    let acpi_handler = AcpiHandlerImpl::new(mmio_allocator);

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
            acpi_handler.clone(),
        )
    };

    unsafe {
        AcpiTables::from_validated_rsdp(acpi_handler, rsdp_mapping)
            .expect("failed to read acpi tables")
    }
}