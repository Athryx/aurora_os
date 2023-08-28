use core::slice::{self, Iter};

use bytemuck::{bytes_of, Pod, Zeroable};

use crate::acpi::rsdt::Rsdt;
use crate::consts;
use crate::mem::PhysAddr;
use crate::prelude::*;
use crate::util::{HwaIter, HwaTag};

// multiboot tag type ids
const END: u32 = 0;
const MODULE: u32 = 3;
const MEMORY_MAP: u32 = 6;
const RSDP_OLD: u32 = 14;
const RSDP_NEW: u32 = 15;

// multiboot memory type ids
// reserved is any other number
const USABLE: u32 = 1;
const ACPI: u32 = 3;
const HIBERNATE_PRESERVE: u32 = 4;
const DEFECTIVE: u32 = 5;

#[repr(C, packed)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct Mb2Start {
    size: u32,
    reserved: u32,
}

impl TrailerInit for Mb2Start {
    fn size(&self) -> usize {
        self.size as usize
    }
}

#[derive(Debug, Clone, Copy)]
enum Mb2Elem<'a> {
    End,
    Module(WithTrailer<'a, Mb2Module>),
    MemoryMap(WithTrailer<'a, Mb2MemoryMapHeader>),
    RsdpOld(Mb2RsdpOld),
    Other(TagHeader),
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct TagHeader {
    typ: u32,
    size: u32,
}

impl HwaTag for TagHeader {
    type Elem<'a> = Mb2Elem<'a>;

    fn size(&self) -> usize {
        self.size as usize
    }

    fn elem(this: WithTrailer<'_, Self>) -> Self::Elem<'_> {
        match this.data.typ {
            END => Mb2Elem::End,
            MODULE => Mb2Elem::Module(Self::data_trailer(&this)),
            MEMORY_MAP => Mb2Elem::MemoryMap(Self::data_trailer(&this)),
            RSDP_OLD => Mb2Elem::RsdpOld(Self::data(&this)),
            RSDP_NEW => todo!(),
            _ => Mb2Elem::Other(this.data),
        }
    }
}

const MAX_MEMORY_REGIONS: usize = 16;

#[derive(Debug, Clone, Copy)]
pub struct MemoryMap {
    data: [MemoryRegionType; MAX_MEMORY_REGIONS],
    len: usize,
}

impl core::ops::Deref for MemoryMap {
    type Target = [MemoryRegionType];

    fn deref(&self) -> &Self::Target {
        unsafe { core::slice::from_raw_parts(&self.data as *const _, self.len) }
    }
}

impl core::ops::DerefMut for MemoryMap {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { core::slice::from_raw_parts_mut(&mut self.data as *mut _, self.len) }
    }
}

impl MemoryMap {
    fn new() -> Self {
        MemoryMap {
            data: [MemoryRegionType::None; MAX_MEMORY_REGIONS],
            len: 0,
        }
    }

    // pushes kernel zone on list if applicable
    fn push(&mut self, region: MemoryRegionType) {
        // this is kind of ugly to do here
        if region.range().addr() == consts::KERNEL_PHYS_RANGE.end_addr() {
            self.push(MemoryRegionType::Kernel(consts::KERNEL_PHYS_RANGE.as_unaligned()));
        }
        assert!(self.len < MAX_MEMORY_REGIONS);
        self.data[self.len] = region;
        self.len += 1;
    }

    pub fn iter(&self) -> Iter<MemoryRegionType> {
        unsafe { slice::from_raw_parts(&self.data as *const _, self.len).iter() }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum MemoryRegionType {
    Usable(UPhysRange),
    Acpi(UPhysRange),
    HibernatePreserve(UPhysRange),
    Defective(UPhysRange),
    Reserved(UPhysRange),
    Kernel(UPhysRange),
    // only used internally, will never be shown if you deref a MemoryMap
    None,
}

impl MemoryRegionType {
    // this one might overlap with the kernel
    unsafe fn new_unchecked(region: &Mb2MemoryRegion) -> Self {
        let prange = UPhysRange::new(PhysAddr::new(region.addr as usize), region.len as usize);

        match region.typ {
            USABLE => Self::Usable(prange),
            ACPI => Self::Acpi(prange),
            HIBERNATE_PRESERVE => Self::HibernatePreserve(prange),
            DEFECTIVE => Self::Defective(prange),
            _ => Self::Reserved(prange),
        }
    }

    fn new(region: &Mb2MemoryRegion, initrd_range: UPhysRange) -> impl Iterator<Item = Self> + '_ {
        let convert_to_memory_region = |prange| match region.typ {
            USABLE => Self::Usable(prange),
            ACPI => Self::Acpi(prange),
            HIBERNATE_PRESERVE => Self::HibernatePreserve(prange),
            DEFECTIVE => Self::Defective(prange),
            _ => Self::Reserved(prange),
        };

        UPhysRange::new(PhysAddr::new(region.addr as usize), region.len as usize)
            .split_at_iter(consts::KERNEL_PHYS_RANGE.as_unaligned())
            .flat_map(move |range| range.split_at_iter(initrd_range))
            .flat_map(|range| range.split_at_iter(consts::AP_CODE_DEST_RANGE.as_unaligned()))
            .map(convert_to_memory_region)
    }

    pub fn range(&self) -> UPhysRange {
        match self {
            Self::Usable(mem) => *mem,
            Self::Acpi(mem) => *mem,
            Self::HibernatePreserve(mem) => *mem,
            Self::Defective(mem) => *mem,
            Self::Reserved(mem) => *mem,
            Self::Kernel(mem) => *mem,
            Self::None => unreachable!(),
        }
    }
}

/// This is the data present after the tag for memory map but before the memory map entries
#[repr(C, packed)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct Mb2MemoryMapHeader {
    entry_size: u32,
    entry_version: u32,
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct Mb2MemoryRegion {
    addr: u64,
    len: u64,
    typ: u32,
    reserved: u32,
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct Mb2Module {
    mod_start: u32,
    mod_end: u32,
}

impl WithTrailer<'_, Mb2Module> {
    fn is_initrd(&self) -> bool {
        self.trailer == "initrd\0".as_bytes()
    }
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct Mb2RsdpOld {
    signature: [u8; 8],
    checksum: u8,
    oemid: [u8; 6],
    revision: u8,
    rsdt_addr: u32,
}

impl Mb2RsdpOld {
    // add up every byte and make sure lowest byte is equal to 0
    fn validate(&self) -> bool {
        let mut sum: usize = 0;
        let slice = bytes_of(self);

        for n in slice {
            sum += *n as usize;
        }

        sum % 0x100 == 0
    }
}

// multiboot 2 structure
#[derive(Debug, Clone, Copy)]
pub struct BootInfo<'a> {
    pub memory_map: MemoryMap,
    pub initrd: &'a [u8],
    pub rsdt: WithTrailer<'a, Rsdt>,
}

impl BootInfo<'_> {
    pub unsafe fn new(addr: usize) -> Self {
        let mb2_data = unsafe {
            WithTrailer::from_pointer(addr as *const Mb2Start)
        };

        let iter: HwaIter<TagHeader> = HwaIter::from_align(mb2_data.trailer, 8);

        let mut initrd_range = None;
        let mut initrd_slice = None;

        let mut memory_map = MemoryMap::new();
        let mut memory_map_tag = None;

        let mut rsdt = None;

        for data in iter {
            match data {
                Mb2Elem::End => break,
                Mb2Elem::Module(data_trailer) => {
                    // look for initrd in module
                    if data_trailer.is_initrd() {
                        let data = data_trailer.data;

                        let size = (data.mod_end - data.mod_start) as usize;
                        let paddr = PhysAddr::new(data.mod_start as usize);
                        initrd_range = Some(UPhysRange::new(paddr, size));

                        let initrd_ptr = paddr.to_virt().as_usize() as *const u8;
                        unsafe {
                            initrd_slice = Some(core::slice::from_raw_parts(initrd_ptr, size));
                        }
                    }
                },
                Mb2Elem::MemoryMap(tag) => memory_map_tag = Some(tag),
                Mb2Elem::RsdpOld(rsdp) => {
                    if !rsdp.validate() {
                        panic!("invalid rsdp passed to kernel");
                    }
                    unsafe {
                        rsdt = Some(
                            WithTrailer::from_pointer(phys_to_virt(rsdp.rsdt_addr as usize) as *const Rsdt)
                        );
                    }
                },
                Mb2Elem::Other(_) => (),
            }
        }

        // have to do this at the end, because it needs to know where multiboot modules are
        if let Some(tag_header) = memory_map_tag {
            for memory_region in iter_unaligned_pod_data(tag_header.trailer) {
                let regions = MemoryRegionType::new(
                    &memory_region,
                    initrd_range.expect("no initrd"),
                );

                for region in regions {
                    memory_map.push(region);
                }
            }
        }

        BootInfo {
            memory_map,
            initrd: initrd_slice.expect("no initrd"),
            rsdt: rsdt.expect("no rsdt"),
        }
    }
}
