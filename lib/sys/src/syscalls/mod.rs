use core::cmp::min;

use bit_utils::Size;

use crate::{syscall_nums::*, CapId, CapType, CapFlags, KResult, CapCloneFlags, CapDestroyFlags};

mod address_space;
pub use address_space::*;
mod allocator;
pub use allocator::*;
mod capability_space;
pub use capability_space::*;
mod channel;
pub use channel::*;
mod key;
pub use key::*;
mod memory;
pub use memory::*;
mod thread;
pub use thread::*;
mod thread_group;
pub use thread_group::*;

// need to use rcx because rbx is reserved by llvm
// FIXME: ugly
#[macro_export]
macro_rules! syscall {
    ($num:expr) => {syscall!($num, 0)};

	($num:expr, $opt:expr) => {{
        core::arch::asm!("syscall",
            inout("rax") (($opt as usize) << 32) | ($num as usize) => _,
            out("rcx") _,
            out("r10") _,
            out("r11") _,
        );
	}};

	($num:expr, $opt:expr, $a1:expr) => {{
		let o1: usize;
        let o2: usize;
        core::arch::asm!("push rbx",
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
        core::arch::asm!("push rbx",
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
        core::arch::asm!("push rbx",
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
        core::arch::asm!("push rbx",
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
        core::arch::asm!("push rbx",
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
        core::arch::asm!("push rbx",
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
        core::arch::asm!("push rbx",
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
        core::arch::asm!("push rbx",
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

#[macro_export]
macro_rules! sysret_0 {
    ($data:expr) => {
        {
            let result = $data;
            let syserr = $crate::SysErr::new(result.0)
                .expect("invalid syserr code recieved from kernel");

            if syserr == $crate::SysErr::Ok {
                Ok(())
            } else {
                Err(syserr)
            }
        }
    };
}

#[macro_export]
macro_rules! sysret_1 {
    ($data:expr) => {
        {
            let result = $data;
            let syserr = $crate::SysErr::new(result.0)
                .expect("invalid syserr code recieved from kernel");

            if syserr == $crate::SysErr::Ok {
                Ok(result.1)
            } else {
                Err(syserr)
            }
        }
    };
}

#[macro_export]
macro_rules! sysret_2 {
    ($data:expr) => {
        {
            let result = $data;
            let syserr = $crate::SysErr::new(result.0)
                .expect("invalid syserr code recieved from kernel");

            if syserr == $crate::SysErr::Ok {
                Ok((result.1, result.2))
            } else {
                Err(syserr)
            }
        }
    };
}

const INVALID_CAPID_MESSAGE: &'static str = "invalid capid recieved from kernel";
const WEAK_AUTO_DESTROY: u32 = 1 << 31;

pub trait Capability {
    const TYPE: CapType;

    /// Create a new capability struct wrapping an existing CapId
    /// 
    /// Returns None if the cap_type of `cap_id` is not the right type
    fn from_cap_id(cap_id: CapId) -> Option<Self>
        where Self: Sized;

    fn cap_id(&self) -> CapId;

    fn as_usize(&self) -> usize {
        self.cap_id().into()
    }
}

/// Specifies which process an operation should be performed on
#[derive(Debug, Clone, Copy)]
pub enum CspaceTarget<'a> {
    /// Perform it on the current process
    Current,
    /// Perform it on another process
    Other(&'a CapabilitySpace),
}

macro_rules! make_cap_fn {
    ($fn_name:ident, $make_weak:expr, $destroy_src_cap:expr) => {
        pub fn $fn_name<T: Capability>(
            dst_cspace: CspaceTarget,
            src_cspace: CspaceTarget,
            cap: T,
            new_flags: CapFlags,
        ) -> KResult<T> {
            let cap_id = cap_clone_inner(
                dst_cspace,
                src_cspace,
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
    dst_cspace: CspaceTarget,
    src_cspace: CspaceTarget,
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

    let src_cspace_id = match src_cspace {
        CspaceTarget::Current => {
            flags |= CapCloneFlags::SRC_CSPACE_SELF;
            0
        },
        CspaceTarget::Other(cspace) => cspace.as_usize(),
    };

    let dst_cspace_id = match dst_cspace {
        CspaceTarget::Current => {
            flags |= CapCloneFlags::DST_CSPACE_SELF;
            0
        },
        CspaceTarget::Other(cspace) => cspace.as_usize(),
    };

    unsafe {
        sysret_1!(syscall!(
            CAP_CLONE,
            flags.bits() | WEAK_AUTO_DESTROY,
            dst_cspace_id,
            src_cspace_id,
            usize::from(cap_id)
        )).map(|num| CapId::try_from(num).expect(INVALID_CAPID_MESSAGE))
    }
}

fn cap_destroy(
    cspace: CspaceTarget,
    capability_id: CapId,
) -> KResult<()> {
    let (cspace_id, flags) = match cspace {
        CspaceTarget::Current => (0, CapDestroyFlags::CSPACE_SELF),
        CspaceTarget::Other(cspace) => (cspace.as_usize(), CapDestroyFlags::empty()),
    };

    unsafe {
        sysret_0!(syscall!(
            CAP_DESTROY,
            flags.bits() | WEAK_AUTO_DESTROY,
            cspace_id,
            usize::from(capability_id)
        ))
    }
}

/// Used for sending and recieving events
#[derive(Debug, Clone, Copy)]
pub struct MessageBuffer<'a> {
    pub memory: &'a Memory,
    pub offset: Size,
    pub size: Size,
}

impl MessageBuffer<'_> {
    pub fn is_readable(&self) -> bool {
        self.memory.cap_id().flags().contains(CapFlags::READ)
    }

    pub fn is_writable(&self) -> bool {
        self.memory.cap_id().flags().contains(CapFlags::WRITE)
    }
}

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