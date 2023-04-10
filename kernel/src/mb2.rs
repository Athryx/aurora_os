use core::slice::{self, Iter};

use crate::acpi::Rsdt;
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
#[derive(Debug, Clone, Copy)]
struct Mb2Start {
    size: u32,
    reserved: u32,
}

impl Mb2Start {
    fn size(&self) -> usize {
        (self.size as usize) - size_of::<Self>()
    }
}

#[derive(Debug, Clone, Copy)]
enum Mb2Elem<'a> {
    End,
    Module(&'a Mb2Module),
    MemoryMap(&'a TagHeader),
    RsdpOld(&'a Mb2RsdpOld),
    Other(&'a TagHeader),
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
struct TagHeader {
    typ: u32,
    size: u32,
}

impl HwaTag for TagHeader {
    type Elem<'a> = Mb2Elem<'a>;

    fn size(&self) -> usize {
        self.size as usize
    }

    fn elem(&self) -> Self::Elem<'_> {
        match self.typ {
            END => Mb2Elem::End,
            MODULE => Mb2Elem::Module(unsafe { self.raw_data() }),
            MEMORY_MAP => Mb2Elem::MemoryMap(self),
            RSDP_OLD => Mb2Elem::RsdpOld(unsafe { self.raw_data() }),
            RSDP_NEW => todo!(),
            _ => Mb2Elem::Other(self),
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

    fn new<'a>(region: &'a Mb2MemoryRegion, initrd_range: UPhysRange) -> impl Iterator<Item = Self> + 'a {
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

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
struct Mb2MemoryRegion {
    addr: u64,
    len: u64,
    typ: u32,
    reserved: u32,
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
struct Mb2Module {
    mod_start: u32,
    mod_end: u32,
}

impl Mb2Module {
    unsafe fn string(&self) -> &str {
        unsafe {
            let ptr = (self as *const Self).add(1) as *const u8;
            from_cstr(ptr).expect("bootloader did not pass valid utf-8 string for module name")
        }
    }
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
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
        let slice = unsafe { slice::from_raw_parts(self as *const _ as *const u8, size_of::<Self>()) };

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
    pub rsdt: &'a Rsdt,
}

impl BootInfo<'_> {
    pub unsafe fn new(addr: usize) -> Self {
        // TODO: use an enum for each tag type, but since I only need memory map for now,
        // that would be a lot of extra typing

        // add 8 to get past initial entry which is always there
        let start = unsafe { (addr as *const Mb2Start).as_ref().unwrap() };
        let iter: HwaIter<TagHeader> =
            unsafe { HwaIter::from_align(addr + size_of::<Mb2Start>(), start.size(), 8) };

        let mut initrd_range = None;
        let mut initrd_slice = None;

        let mut memory_map = MemoryMap::new();
        let mut memory_map_tag = None;

        let mut rsdt = None;

        for data in iter {
            match data {
                Mb2Elem::End => break,
                Mb2Elem::Module(data) => {
                    // look for initrd in module
                    if unsafe { data.string() } == "initrd" {
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
                        rsdt = Rsdt::from(rsdp.rsdt_addr as usize);
                    }
                },
                Mb2Elem::Other(_) => (),
            }
        }

        // have to do this at the end, because it needs to know where multiboot modules are
        if let Some(tag_header) = memory_map_tag {
            let mut ptr = unsafe { (tag_header as *const _ as *const u8).add(16) as *const Mb2MemoryRegion };

            let len = (tag_header.size - 16) / 24;

            for _ in 0..len {
                let region = unsafe { ptr.as_ref().unwrap() };

                let regions = MemoryRegionType::new(region, initrd_range.expect("no initrd"));

                for region in regions {
                    memory_map.push(region);
                }

                unsafe {
                    ptr = ptr.add(1);
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
