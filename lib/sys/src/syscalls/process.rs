use serde::{Serialize, Deserialize};
use bit_utils::Size;

use crate::{
    CapId,
    CapType,
    CapFlags,
    KResult,
    Tid,
    MemoryResizeFlags,
    MemoryMappingFlags,
    MemoryMapFlags,
    MemoryUpdateMappingFlags,
    syscall,
    sysret_0,
    sysret_1,
};
use crate::syscall_nums::*;
use super::{Capability, Allocator, Spawner, Memory, WEAK_AUTO_DESTROY, INVALID_CAPID_MESSAGE};

pub fn thread_yield() {
    unsafe {
        syscall!(
            THREAD_YIELD,
            0
        );
    }
}

pub fn suspend() {
    unsafe {
        syscall!(
            THREAD_SUSPEND,
            0
        );
    }
}

pub fn suspend_until(nsec: u64) {
    unsafe {
        syscall!(
            THREAD_SUSPEND,
            1,
            nsec
        );
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Process(CapId);

impl Capability for Process {
    const TYPE: CapType = CapType::Process;

    fn from_cap_id(cap_id: CapId) -> Option<Self> {
        if cap_id.cap_type() == CapType::Process {
            Some(Process(cap_id))
        } else {
            None
        }
    }

    fn cap_id(&self) -> CapId {
        self.0
    }
}

impl Process {
    pub fn new(flags: CapFlags, allocator: Allocator, spawner: Spawner) -> KResult<Self> {
        unsafe {
            sysret_1!(syscall!(
                PROCESS_NEW,
                flags.bits() as u32 | WEAK_AUTO_DESTROY,
                allocator.as_usize(),
                spawner.as_usize()
            )).map(|num| Process(CapId::try_from(num).expect(INVALID_CAPID_MESSAGE)))
        }
    }

    pub fn exit(&self) -> KResult<()> {
        unsafe {
            sysret_0!(syscall!(
                PROCESS_EXIT,
                WEAK_AUTO_DESTROY,
                self.as_usize()
            ))
        }
    }

    pub fn thread_new(&self, autostart_thread: bool, rip: usize, rsp: usize, regs: (usize, usize, usize, usize)) -> KResult<Tid> {
        unsafe {
            sysret_1!(syscall!(
                THREAD_NEW,
                autostart_thread as u32 | WEAK_AUTO_DESTROY,
                self.as_usize(),
                rip,
                rsp,
                regs.0,
                regs.1,
                regs.2,
                regs.3
            )).map(Tid::from)
        }
    }

    pub fn map_memory(&self, memory: Memory, address: usize, max_size: Option<Size>, flags: MemoryMappingFlags) -> KResult<usize> {
        let mut flags = flags.bits() | WEAK_AUTO_DESTROY;
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
                max_size.unwrap_or_default().pages_rounded()
            ))
        }
    }

    pub fn unmap_memory(&self, memory: Memory) -> KResult<()> {
        unsafe {
            sysret_0!(syscall!(
                MEMORY_UNMAP,
                WEAK_AUTO_DESTROY,
                self.as_usize(),
                memory.as_usize()
            ))
        }
    }

    pub fn update_memory_mapping(&self, memory: Memory, new_map_size: Option<Size>) -> KResult<usize> {
        let mut flags = MemoryUpdateMappingFlags::empty();
        if new_map_size.is_some() {
            flags |= MemoryUpdateMappingFlags::UPDATE_SIZE;
        }

        unsafe {
            sysret_1!(syscall!(
                MEMORY_UPDATE_MAPPING,
                flags.bits() | WEAK_AUTO_DESTROY,
                self.as_usize(),
                memory.as_usize(),
                new_map_size.unwrap_or_default().pages_rounded()
            ))
        }
    }

    pub fn resize_memory(&self, memory: &mut Memory, new_size: Size, flags: MemoryResizeFlags) -> KResult<usize> {
        let new_size = unsafe {
            sysret_1!(syscall!(
                MEMORY_RESIZE,
                flags.bits(),
                self.as_usize(),
                memory.as_usize(),
                new_size.pages_rounded()
            ))
        }?;

        // panic safety: from_pages can panic, but syscall should not return invalid number of pages
        memory.size = Size::from_pages(new_size);

        Ok(new_size)
    }
}