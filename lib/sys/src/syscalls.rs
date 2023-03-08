use core::arch::asm;
use core::cmp::min;

use crate::{syscall_nums::*, CapId, CapType, CapFlags, SysErr, KResult, Tid};

// need to use rcx because rbx is reserved by llvm
// FIXME: ugly
macro_rules! syscall
{
	($num:expr, $opt:expr) => {{
        unsafe {
		    asm!("syscall", inout("rax") (($opt as usize) << 32) | ($num as usize) => _);
        }
	}};

	($num:expr, $opt:expr, $a1:expr) => {{
		let o1: usize;
        unsafe {
            asm!("push rbx",
                "mov rbx, rcx",
                "syscall",
                "mov rcx, rbx",
                "pop rbx",
                inout("rax") (($opt as usize) << 32) | ($num as usize) => _,
                inout("rcx") $a1 => o1,
                );
        }
		o1
	}};

	($num:expr, $opt:expr, $a1:expr, $a2:expr) => {{
		let o1: usize;
		let o2: usize;
        unsafe {
            asm!("push rbx",
                "mov rbx, rcx",
                "syscall",
                "mov rcx, rbx",
                "pop rbx",
                inout("rax") (($opt as usize) << 32) | ($num as usize) => _,
                inout("rcx") $a1 => o1,
                inout("rdx") $a2 => o2,
                );
        }
		(o1, o2)
	}};

	($num:expr, $opt:expr, $a1:expr, $a2:expr, $a3:expr) => {{
		let o1: usize;
		let o2: usize;
		let o3: usize;
        unsafe {
            asm!("push rbx",
                "mov rbx, rcx",
                "syscall",
                "mov rcx, rbx",
                "pop rbx",
                inout("rax") (($opt as usize) << 32) | ($num as usize) => _,
                inout("rcx") $a1 => o1,
                inout("rdx") $a2 => o2,
                inout("rsi") $a3 => o3,
                );
        }
		(o1, o2, o3)
	}};

	($num:expr, $opt:expr, $a1:expr, $a2:expr, $a3:expr, $a4:expr) => {{
		let o1: usize;
		let o2: usize;
		let o3: usize;
		let o4: usize;
        unsafe {
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
                );
        }
		(o1, o2, o3, o4)
	}};

	($num:expr, $opt:expr, $a1:expr, $a2:expr, $a3:expr, $a4:expr, $a5:expr) => {{
		let o1: usize;
		let o2: usize;
		let o3: usize;
		let o4: usize;
		let o5: usize;
        unsafe {
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
                inout("r8") $a5 => o5,
                );
        }
		(o1, o2, o3, o4, o5)
	}};

	($num:expr, $opt:expr, $a1:expr, $a2:expr, $a3:expr, $a4:expr, $a5:expr, $a6:expr) => {{
		let o1: usize;
		let o2: usize;
		let o3: usize;
		let o4: usize;
		let o5: usize;
		let o6: usize;
        unsafe {
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
                inout("r8") $a5 => o5,
                inout("r9") $a6 => o6,
                );
        }
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
        unsafe {
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
                inout("r8") $a5 => o5,
                inout("r9") $a6 => o6,
                inout("r12") $a7 => o7,
                );
        }
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
        unsafe {
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
                inout("r8") $a5 => o5,
                inout("r9") $a6 => o6,
                inout("r12") $a7 => o7,
                inout("r13") $a8 => o8,
                );
        }
		(o1, o2, o3, o4, o5, o6, o7, o8)
	}};

	($num:expr, $opt:expr, $a1:expr, $a2:expr, $a3:expr, $a4:expr, $a5:expr, $a6:expr, $a7:expr, $a8:expr, $a9:expr) => {{
		let o1: usize;
		let o2: usize;
		let o3: usize;
		let o4: usize;
		let o5: usize;
		let o6: usize;
		let o7: usize;
		let o8: usize;
		let o9: usize;
        unsafe {
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
                inout("r8") $a5 => o5,
                inout("r9") $a6 => o6,
                inout("r12") $a7 => o7,
                inout("r13") $a8 => o8,
                inout("r14") $a9 => o9,
                );
        }
		(o1, o2, o3, o4, o5, o6, o7, o8, o9)
	}};

	($num:expr, $opt:expr, $a1:expr, $a2:expr, $a3:expr, $a4:expr, $a5:expr, $a6:expr, $a7:expr, $a8:expr, $a9:expr, $a10:expr) => {{
		let o1: usize;
		let o2: usize;
		let o3: usize;
		let o4: usize;
		let o5: usize;
		let o6: usize;
		let o7: usize;
		let o8: usize;
		let o9: usize;
		let o10: usize;
        unsafe {
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
                inout("r8") $a5 => o5,
                inout("r9") $a6 => o6,
                inout("r12") $a7 => o7,
                inout("r13") $a8 => o8,
                inout("r14") $a9 => o9,
                inout("r15") $a10 => o10,
                );
        }
		(o1, o2, o3, o4, o5, o6, o7, o8, o9, o10)
	}};
}

const INVALID_SYSERR_MESSAGE: &'static str = "invalid syserr code recieved from kernel";
const INVALID_CAPID_MESSAGE: &'static str = "invalid capid recieved from kernel";

macro_rules! sysret_0 {
    ($data:expr) => {
        {
            let result = $data;
            let syserr = SysErr::new(result)
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
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

const WEAK_AUTO_DESTROY: usize = 1 << 31;

/// Prints up to 80 bytes from the input array to the kernel debug log
pub fn print_debug(data: &[u8]) {
    let num_chars = min(80, data.len());

    let get_char = |n| *data.get(n).unwrap_or(&0) as usize;

    let get_arg = |arg: usize| {
        let base = arg * 8;

        get_char(0)
            | get_char(base + 1) << 8
            | get_char(base + 2) << 16
            | get_char(base + 3) << 24
            | get_char(base + 4) << 32
            | get_char(base + 5) << 40
            | get_char(base + 6) << 48
            | get_char(base + 7) << 56
    };

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
        get_arg(7),
        get_arg(8),
        get_arg(9)
    );
}

make_cap_struct!(Process, CapType::Process);

impl Process {
    pub fn new(flags: CapFlags, allocator: Allocator, spawner: Spawner) -> KResult<Self> {
        sysret_1!(syscall!(
            PROCESS_NEW,
            flags.bits() | WEAK_AUTO_DESTROY,
            allocator.as_usize(),
            spawner.as_usize()
        )).map(|num| Process(CapId::try_from(num).expect(INVALID_CAPID_MESSAGE)))
    }

    pub fn exit(&self) -> KResult<()> {
        sysret_0!(syscall!(
            PROCESS_EXIT,
            WEAK_AUTO_DESTROY,
            self.as_usize()
        ))
    }

    pub fn thread_new(&self, autostart_thread: bool, rip: usize, rsp: usize, regs: (usize, usize, usize, usize)) -> KResult<Tid> {
        sysret_1!(syscall!(
            THREAD_NEW,
            autostart_thread as usize | WEAK_AUTO_DESTROY,
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

make_cap_struct!(Memory, CapType::Memory);

make_cap_struct!(Key, CapType::Key);

make_cap_struct!(Spawner, CapType::Spawner);

make_cap_struct!(Allocator, CapType::Allocator);