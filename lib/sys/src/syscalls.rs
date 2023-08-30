use core::arch::asm;
use core::cmp::min;

use bit_utils::Size;
use serde::{Serialize, Deserialize};

use crate::{syscall_nums::*, CapId, CapType, CapFlags, SysErr, KResult, Tid, MemoryResizeFlags, MemoryMappingFlags, MemoryMapFlags, MemoryUpdateMappingFlags, ChannelSyncFlags};

// need to use rcx because rbx is reserved by llvm
// FIXME: ugly
macro_rules! syscall {
    ($num:expr) => {syscall!($num, 0)};

	($num:expr, $opt:expr) => {{
        asm!("syscall",
            inout("rax") (($opt as usize) << 32) | ($num as usize) => _,
            out("rcx") _,
            out("r10") _,
            out("r11") _,
        );
	}};

	($num:expr, $opt:expr, $a1:expr) => {{
		let o1: usize;
        let o2: usize;
        asm!("push rbx",
            "mov rbx, rcx",
            "syscall",
            "mov rcx, rbx",
            "pop rbx",
            inout("rax") (($opt as usize) << 32) | ($num as usize) => _,
            inout("rcx") $a1 => o1,
            out("rdx") o2,
            out("r10") _,
            out("r11") _,
        );
		(o1, o2)
	}};

	($num:expr, $opt:expr, $a1:expr, $a2:expr) => {{
		let o1: usize;
		let o2: usize;
        asm!("push rbx",
            "mov rbx, rcx",
            "syscall",
            "mov rcx, rbx",
            "pop rbx",
            inout("rax") (($opt as usize) << 32) | ($num as usize) => _,
            inout("rcx") $a1 => o1,
            inout("rdx") $a2 => o2,
            out("r10") _,
            out("r11") _,
        );
		(o1, o2)
	}};

	($num:expr, $opt:expr, $a1:expr, $a2:expr, $a3:expr) => {{
		let o1: usize;
		let o2: usize;
		let o3: usize;
        asm!("push rbx",
            "mov rbx, rcx",
            "syscall",
            "mov rcx, rbx",
            "pop rbx",
            inout("rax") (($opt as usize) << 32) | ($num as usize) => _,
            inout("rcx") $a1 => o1,
            inout("rdx") $a2 => o2,
            inout("rsi") $a3 => o3,
            out("r10") _,
            out("r11") _,
        );
		(o1, o2, o3)
	}};

	($num:expr, $opt:expr, $a1:expr, $a2:expr, $a3:expr, $a4:expr) => {{
		let o1: usize;
		let o2: usize;
		let o3: usize;
		let o4: usize;
        asm!("push rbx",
            "mov rbx, rcx",
            "syscall",
            "mov rcx, rbx",
            "pop rbx",
            inout("rax") (($opt as usize) << 32) | ($num as usize) => _,
            inout("rcx") $a1 => o1,
            inout("rdx") $a2 => o2,
            inout("rsi") $a3 => o3,
            inout("rdi") $a4 => o4,
            out("r10") _,
            out("r11") _,
        );
		(o1, o2, o3, o4)
	}};

	($num:expr, $opt:expr, $a1:expr, $a2:expr, $a3:expr, $a4:expr, $a5:expr) => {{
		let o1: usize;
		let o2: usize;
		let o3: usize;
		let o4: usize;
		let o5: usize;
        asm!("push rbx",
            "mov rbx, rcx",
            "syscall",
            "mov rcx, rbx",
            "pop rbx",
            inout("rax") (($opt as usize) << 32) | ($num as usize) => _,
            inout("rcx") $a1 => o1,
            inout("rdx") $a2 => o2,
            inout("rsi") $a3 => o3,
            inout("rdi") $a4 => o4,
            inout("r12") $a5 => o5,
            out("r10") _,
            out("r11") _,
        );
		(o1, o2, o3, o4, o5)
	}};

	($num:expr, $opt:expr, $a1:expr, $a2:expr, $a3:expr, $a4:expr, $a5:expr, $a6:expr) => {{
		let o1: usize;
		let o2: usize;
		let o3: usize;
		let o4: usize;
		let o5: usize;
		let o6: usize;
        asm!("push rbx",
            "mov rbx, rcx",
            "syscall",
            "mov rcx, rbx",
            "pop rbx",
            inout("rax") (($opt as usize) << 32) | ($num as usize) => _,
            inout("rcx") $a1 => o1,
            inout("rdx") $a2 => o2,
            inout("rsi") $a3 => o3,
            inout("rdi") $a4 => o4,
            inout("r12") $a5 => o5,
            inout("r13") $a6 => o6,
            out("r10") _,
            out("r11") _,
        );
		(o1, o2, o3, o4, o5, o6)
	}};

	($num:expr, $opt:expr, $a1:expr, $a2:expr, $a3:expr, $a4:expr, $a5:expr, $a6:expr, $a7:expr) => {{
		let o1: usize;
		let o2: usize;
		let o3: usize;
		let o4: usize;
		let o5: usize;
		let o6: usize;
		let o7: usize;
        asm!("push rbx",
            "mov rbx, rcx",
            "syscall",
            "mov rcx, rbx",
            "pop rbx",
            inout("rax") (($opt as usize) << 32) | ($num as usize) => _,
            inout("rcx") $a1 => o1,
            inout("rdx") $a2 => o2,
            inout("rsi") $a3 => o3,
            inout("rdi") $a4 => o4,
            inout("r12") $a5 => o5,
            inout("r13") $a6 => o6,
            inout("r14") $a7 => o7,
            out("r10") _,
            out("r11") _,
        );
		(o1, o2, o3, o4, o5, o6, o7)
	}};

	($num:expr, $opt:expr, $a1:expr, $a2:expr, $a3:expr, $a4:expr, $a5:expr, $a6:expr, $a7:expr, $a8:expr) => {{
		let o1: usize;
		let o2: usize;
		let o3: usize;
		let o4: usize;
		let o5: usize;
		let o6: usize;
		let o7: usize;
		let o8: usize;
        asm!("push rbx",
            "mov rbx, rcx",
            "syscall",
            "mov rcx, rbx",
            "pop rbx",
            inout("rax") (($opt as usize) << 32) | ($num as usize) => _,
            inout("rcx") $a1 => o1,
            inout("rdx") $a2 => o2,
            inout("rsi") $a3 => o3,
            inout("rdi") $a4 => o4,
            inout("r12") $a5 => o5,
            inout("r13") $a6 => o6,
            inout("r14") $a7 => o7,
            inout("r15") $a8 => o8,
            out("r10") _,
            out("r11") _,
        );
		(o1, o2, o3, o4, o5, o6, o7, o8)
	}};
}

const INVALID_SYSERR_MESSAGE: &'static str = "invalid syserr code recieved from kernel";
const INVALID_CAPID_MESSAGE: &'static str = "invalid capid recieved from kernel";

macro_rules! sysret_0 {
    ($data:expr) => {
        {
            let result = $data;
            let syserr = SysErr::new(result.0)
                .expect(INVALID_SYSERR_MESSAGE);

            if syserr == SysErr::Ok {
                Ok(())
            } else {
                Err(syserr)
            }
        }
    };
}

macro_rules! sysret_1 {
    ($data:expr) => {
        {
            let result = $data;
            let syserr = SysErr::new(result.0)
                .expect(INVALID_SYSERR_MESSAGE);

            if syserr == SysErr::Ok {
                Ok(result.1)
            } else {
                Err(syserr)
            }
        }
    };
}

macro_rules! sysret_2 {
    ($data:expr) => {
        {
            let result = $data;
            let syserr = SysErr::new(result.0)
                .expect(INVALID_SYSERR_MESSAGE);

            if syserr == SysErr::Ok {
                Ok((result.1, result.2))
            } else {
                Err(syserr)
            }
        }
    };
}

macro_rules! make_cap_struct {
    ($name:ident, $type:expr) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
        pub struct $name(CapId);

        impl $name {
            /// Create a new capability struct wrapping an existing CapId
            /// 
            /// Returns None if the cap_type of `cap_id` is not the right type
            pub fn try_from(cap_id: CapId) -> Option<Self> {
                if cap_id.cap_type() == $type {
                    Some(Self(cap_id))
                } else {
                    None
                }
            }
        
            /// Returns the CapId of this capability struct
            pub fn cap_id(&self) -> CapId {
                self.0
            }

            pub fn as_usize(&self) -> usize {
                self.0.into()
            }
        }

        impl From<$name> for usize {
            fn from(cap: $name) -> usize {
                cap.0.into()
            }
        }
    };
}

const WEAK_AUTO_DESTROY: u32 = 1 << 31;

/// Prints up to 64 bytes from the input array to the kernel debug log
fn print_debug_inner(data: &[u8]) {
    let num_chars = min(64, data.len());

    let get_char = |n| *data.get(n).unwrap_or(&0) as usize;

    let get_arg = |arg: usize| {
        let base = arg * 8;

        get_char(base)
            | get_char(base + 1) << 8
            | get_char(base + 2) << 16
            | get_char(base + 3) << 24
            | get_char(base + 4) << 32
            | get_char(base + 5) << 40
            | get_char(base + 6) << 48
            | get_char(base + 7) << 56
    };

    unsafe {
        syscall!(
            PRINT_DEBUG,
            num_chars,
            get_arg(0),
            get_arg(1),
            get_arg(2),
            get_arg(3),
            get_arg(4),
            get_arg(5),
            get_arg(6),
            get_arg(7)
        );
    }
}

/// Prints `data` to the kernel debug log
pub fn debug_print(data: &[u8]) {
    for chunk in data.chunks(64) {
        print_debug_inner(chunk);
    }
}

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

make_cap_struct!(Process, CapType::Process);

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Memory {
    id: CapId,
    /// Size of memory
    size: Size,
}

impl Memory {
    /// Updates the size field using `memory_get_size` syscall
    /// 
    /// # Returns
    /// 
    /// The new size of the memory in pages
    pub fn refresh_size(&mut self) -> KResult<Size> {
        // panic safety: from_pages can panic, but syscall should not return invalid number of pages
        self.size = unsafe {
            Size::from_pages(sysret_1!(syscall!(
                MEMORY_GET_SIZE,
                WEAK_AUTO_DESTROY,
                self.as_usize()
            ))?)
        };

        Ok(self.size)
    }

    /// Create a new capability struct wrapping an existing CapId
    /// 
    /// Returns None if the cap_type of `cap_id` is not the right type
    pub fn try_from(cap_id: CapId) -> Option<Self> {
        if cap_id.cap_type() == CapType::Memory {
            let mut out = Self {
                id: cap_id,
                size: Size::default(),
            };

            out.refresh_size().ok()?;

            Some(out)
        } else {
            None
        }
    }

    pub fn size(&self) -> Size {
        self.size
    }

    /// Returns the CapId of this capability struct
    pub fn cap_id(&self) -> CapId {
        self.id
    }

    pub fn as_usize(&self) -> usize {
        self.id.into()
    }
}

impl From<Memory> for usize {
    fn from(cap: Memory) -> usize {
        cap.id.into()
    }
}

impl Memory {
    pub fn new(flags: CapFlags, allocator: Allocator, size: Size) -> KResult<Self> {
        unsafe {
            sysret_2!(syscall!(
                MEMORY_NEW,
                flags.bits() as u32 | WEAK_AUTO_DESTROY,
                allocator.as_usize(),
                size.pages_rounded(),
                // FIXME: hack to make syscall macro return right amount of values
                0 as usize
            )).map(|(cap_id, size)| Memory {
                id: CapId::try_from(cap_id).expect(INVALID_CAPID_MESSAGE),
                size: Size::from_pages(size),
            })
        }
    }
}

/// Used for sending and recieving events
#[derive(Debug, Clone, Copy)]
pub struct MessageBuffer {
    pub memory: Memory,
    pub offset: Size,
    pub size: Size,
}

impl MessageBuffer {
    pub fn is_readable(&self) -> bool {
        self.memory.cap_id().flags().contains(CapFlags::READ)
    }

    pub fn is_writable(&self) -> bool {
        self.memory.cap_id().flags().contains(CapFlags::WRITE)
    }
}

make_cap_struct!(Channel, CapType::Channel);

impl Channel {
    pub fn new(flags: CapFlags, allocator: Allocator) -> KResult<Self> {
        unsafe {
            sysret_1!(syscall!(
                CHANNEL_NEW,
                flags.bits() as u32 | WEAK_AUTO_DESTROY,
                allocator.as_usize()
            )).map(|num| Channel(CapId::try_from(num).expect(INVALID_CAPID_MESSAGE)))
        }
    }

    pub fn try_send(&self, buffer: &MessageBuffer) -> KResult<Size> {
        assert!(buffer.is_readable());

        unsafe {
            sysret_1!(syscall!(
                CHANNEL_TRY_SEND,
                WEAK_AUTO_DESTROY,
                self.as_usize(),
                buffer.memory.as_usize(),
                buffer.offset.bytes(),
                buffer.size.bytes()
            )).map(Size::from_bytes)
        }
    }

    pub fn sync_send(&self, buffer: &MessageBuffer, timeout: Option<u64>) -> KResult<Size> {
        assert!(buffer.is_readable());

        let flags = match timeout {
            Some(_) => ChannelSyncFlags::TIMEOUT,
            None => ChannelSyncFlags::empty(),
        };

        unsafe {
            sysret_1!(syscall!(
                CHANNEL_SYNC_SEND,
                flags.bits() as u32 | WEAK_AUTO_DESTROY,
                self.as_usize(),
                buffer.memory.as_usize(),
                buffer.offset.bytes(),
                buffer.size.bytes(),
                timeout.unwrap_or_default()
            )).map(Size::from_bytes)
        }
    }

    pub fn try_recv(&self, buffer: &MessageBuffer) -> KResult<Size> {
        assert!(buffer.is_writable());

        unsafe {
            sysret_1!(syscall!(
                CHANNEL_TRY_RECV,
                WEAK_AUTO_DESTROY,
                self.as_usize(),
                buffer.memory.as_usize(),
                buffer.offset.bytes(),
                buffer.size.bytes()
            )).map(Size::from_bytes)
        }
    }

    pub fn sync_recv(&self, buffer: &MessageBuffer, timeout: Option<u64>) -> KResult<Size> {
        assert!(buffer.is_writable());

        let flags = match timeout {
            Some(_) => ChannelSyncFlags::TIMEOUT,
            None => ChannelSyncFlags::empty(),
        };

        unsafe {
            sysret_1!(syscall!(
                CHANNEL_SYNC_RECV,
                flags.bits() as u32 | WEAK_AUTO_DESTROY,
                self.as_usize(),
                buffer.memory.as_usize(),
                buffer.offset.bytes(),
                buffer.size.bytes(),
                timeout.unwrap_or_default()
            )).map(Size::from_bytes)
        }
    }
}

make_cap_struct!(Key, CapType::Key);

impl Key {
    pub fn new(flags: CapFlags, allocator: Allocator) -> KResult<Self> {
        unsafe {
            sysret_1!(syscall!(
                KEY_NEW,
                flags.bits() as u32 | WEAK_AUTO_DESTROY,
                allocator.as_usize()
            )).map(|num| Key(CapId::try_from(num).expect(INVALID_CAPID_MESSAGE)))
        }
    }

    pub fn key_id(&self) -> KResult<usize> {
        unsafe {
            sysret_1!(syscall!(
                KEY_ID,
                WEAK_AUTO_DESTROY,
                self.as_usize()
            ))
        }
    }
}

make_cap_struct!(Spawner, CapType::Spawner);

impl Spawner {
    pub fn new(flags: CapFlags, allocator: Allocator, spawn_key: Key) -> KResult<Self> {
        unsafe {
            sysret_1!(syscall!(
                SPAWNER_NEW,
                flags.bits() as u32 | WEAK_AUTO_DESTROY,
                allocator.as_usize(),
                spawn_key.as_usize()
            )).map(|num| Spawner(CapId::try_from(num).expect(INVALID_CAPID_MESSAGE)))
        }
    }

    pub fn kill_all(&self) -> KResult<()> {
        unsafe {
            sysret_0!(syscall!(
                SPAWNER_KILL_ALL,
                WEAK_AUTO_DESTROY,
                self.as_usize()
            ))
        }
    }
}

make_cap_struct!(Allocator, CapType::Allocator);