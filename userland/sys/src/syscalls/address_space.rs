use serde::{Serialize, Deserialize};
use bit_utils::Size;

use crate::{
    CapId,
    CapType,
    KResult,
    MemoryMappingFlags,
    MemoryMapFlags,
    MemoryUpdateMappingFlags,
    CspaceTarget,
    syscall,
    sysret_0,
    sysret_1, MemoryCacheSetting,
};
use crate::syscall_nums::*;
use super::{Capability, Allocator, Memory, EventPool, PhysMem, cap_destroy, WEAK_AUTO_DESTROY, INVALID_CAPID_MESSAGE};

#[derive(Debug, Serialize, Deserialize)]
pub struct AddressSpace(CapId);

impl Capability for AddressSpace {
    const TYPE: CapType = CapType::AddressSpace;

    fn cloned_new_id(&self, cap_id: CapId) -> Option<Self> {
        Self::from_cap_id(cap_id)
    }

    fn cap_id(&self) -> CapId {
        self.0
    }
}

impl Drop for AddressSpace {
    fn drop(&mut self) {
        let _ = cap_destroy(CspaceTarget::Current, self.0);
    }
}

#[derive(Debug, Clone, Copy)]
pub struct MemoryMappingOptions {
    pub read: bool,
    pub write: bool,
    pub exec: bool,
    pub cacheing: MemoryCacheSetting,
}

impl Default for MemoryMappingOptions {
    fn default() -> Self {
        MemoryMappingOptions {
            read: true,
            write: true,
            exec: false,
            cacheing: MemoryCacheSetting::default(),
        }
    }
}

impl From<MemoryMappingOptions> for MemoryMappingFlags {
    fn from(value: MemoryMappingOptions) -> Self {
        let mut out = value.cacheing.into();

        if value.read {
            out |= Self::READ;
        }
        if value.write {
            out |= Self::WRITE;
        }
        if value.exec {
            out |= Self::EXEC
        }

        out
    }
}

impl AddressSpace {
    pub fn from_cap_id(cap_id: CapId) -> Option<Self> {
        if cap_id.cap_type() == CapType::AddressSpace {
            Some(AddressSpace(cap_id))
        } else {
            None
        }
    }

    pub fn new(allocator: &Allocator) -> KResult<Self> {
        let addr_space_id = unsafe {
            sysret_1!(syscall!(
                ADDRESS_SPACE_NEW,
                WEAK_AUTO_DESTROY,
                allocator.as_usize()
            ))?
        };

        Ok(AddressSpace(
            CapId::try_from(addr_space_id)
                .expect(INVALID_CAPID_MESSAGE)
        ))
    }

    pub fn map_memory(&self, memory: &Memory, address: usize, max_size: Option<Size>, map_offset: Size, args: MemoryMappingOptions) -> KResult<Size> {
        let mut flags = MemoryMappingFlags::from(args).bits() | WEAK_AUTO_DESTROY;
        if max_size.is_some() {
            flags |= MemoryMapFlags::MAX_SIZE.bits()
        }

        unsafe {
            sysret_1!(syscall!(
                MEMORY_MAP,
                flags,
                self.as_usize(),
                memory.as_usize(),
                address,
                max_size.unwrap_or_default().pages_rounded(),
                map_offset.pages_rounded()
            )).map(Size::from_pages)
        }
    }

    pub fn map_event_pool(&self, event_pool: &EventPool, address: usize) -> KResult<Size> {
        unsafe {
            sysret_1!(syscall!(
                EVENT_POOL_MAP,
                WEAK_AUTO_DESTROY,
                self.as_usize(),
                event_pool.as_usize(),
                address
            )).map(Size::from_pages)
        }
    }

    pub fn map_phys_mem(&self, phys_mem: &PhysMem, address: usize, args: MemoryMappingOptions) -> KResult<Size> {
        unsafe {
            sysret_1!(syscall!(
                PHYS_MEM_MAP,
                MemoryMappingFlags::from(args).bits() | WEAK_AUTO_DESTROY,
                self.as_usize(),
                phys_mem.as_usize(),
                address
            )).map(Size::from_pages)
        }
    }

    pub fn unmap(&self, address: usize) -> KResult<()> {
        unsafe {
            sysret_0!(syscall!(
                ADDRESS_SPACE_UNMAP,
                WEAK_AUTO_DESTROY,
                self.as_usize(),
                address
            ))
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub enum UpdateVal<T> {
    Change(T),
    #[default]
    KeepSame,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct UpdateMappingArgs {
    pub map_size: UpdateVal<Option<Size>>,
    pub flags: UpdateVal<MemoryMappingFlags>,
}

impl AddressSpace {
    pub fn update_memory_mapping(&self, address: usize, args: UpdateMappingArgs) -> KResult<Size> {
        let mut flags = MemoryUpdateMappingFlags::empty();

        let map_size = if let UpdateVal::Change(map_size) = args.map_size {
            flags |= MemoryUpdateMappingFlags::UPDATE_SIZE;
            if let Some(map_size) = map_size {
                flags |= MemoryUpdateMappingFlags::EXACT_SIZE;
                map_size
            } else {
                Size::zero()
            }
        } else {
            Size::zero()
        };

        let map_flags = if let UpdateVal::Change(map_flags) = args.flags {
            flags |= MemoryUpdateMappingFlags::UPDATE_FLAGS;
            map_flags
        } else {
            MemoryMappingFlags::empty()
        };

        unsafe {
            sysret_1!(syscall!(
                MEMORY_UPDATE_MAPPING,
                map_flags.bits() | flags.bits() | WEAK_AUTO_DESTROY,
                self.as_usize(),
                address,
                map_size.pages_rounded()
            )).map(Size::from_pages)
        }
    }
}