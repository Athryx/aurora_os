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
    CapCloneFlags, 
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


/// Specifies which process an operation should be performed on
#[derive(Debug, Clone, Copy)]
pub enum ProcessTarget {
    /// Perform it on the current process
    Current,
    /// Perform it on another process
    Other(Process),
}

macro_rules! make_cap_fn {
    ($fn_name:ident, $make_weak:expr, $destroy_src_cap:expr) => {
        pub fn $fn_name<T: Capability>(
            dst_process: ProcessTarget,
            src_process: ProcessTarget,
            cap: T,
            new_flags: CapFlags,
        ) -> KResult<T> {
            let cap_id = cap_clone_inner(
                dst_process,
                src_process,
                cap.cap_id(),
                new_flags,
                $make_weak,
                $destroy_src_cap,
            )?;

            Ok(T::from_cap_id(cap_id).expect("invalid capid returned by kernel"))
        }        
    };
}

make_cap_fn!(cap_clone, false, false);
make_cap_fn!(cap_move, false, true);
make_cap_fn!(cap_clone_weak, true, false);
make_cap_fn!(cap_move_weak, true, true);

fn cap_clone_inner(
    dst_process: ProcessTarget,
    src_process: ProcessTarget,
    cap_id: CapId,
    new_flags: CapFlags,
    make_weak: bool,
    destroy_src_cap: bool,
) -> KResult<CapId> {
    let mut flags = CapCloneFlags::empty();

    if new_flags.contains(CapFlags::READ) {
        flags |= CapCloneFlags::READ;
    }
    if new_flags.contains(CapFlags::PROD) {
        flags |= CapCloneFlags::PROD;
    }
    if new_flags.contains(CapFlags::WRITE) {
        flags |= CapCloneFlags::WRITE;
    }
    if new_flags.contains(CapFlags::UPGRADE) {
        flags |= CapCloneFlags::UPGRADE;
    }

    if make_weak {
        flags |= CapCloneFlags::MAKE_WEAK;
    }

    if destroy_src_cap {
        flags |= CapCloneFlags::DESTROY_SRC_CAP;
    }

    let src_process_id = match src_process {
        ProcessTarget::Current => {
            flags |= CapCloneFlags::SRC_PROCESS_SELF;
            0
        },
        ProcessTarget::Other(process) => process.as_usize(),
    };

    let dst_process_id = match dst_process {
        ProcessTarget::Current => {
            flags |= CapCloneFlags::DST_PROCESS_SELF;
            0
        },
        ProcessTarget::Other(process) => process.as_usize(),
    };

    unsafe {
        sysret_1!(syscall!(
            CAP_CLONE,
            flags.bits() | WEAK_AUTO_DESTROY,
            dst_process_id,
            src_process_id,
            usize::from(cap_id)
        )).map(|num| CapId::try_from(num).expect(INVALID_CAPID_MESSAGE))
    }
}