use bytemuck::Pod;
use sys::syscall_nums::*;

use crate::consts::KERNEL_VMA;
use crate::prelude::*;
use crate::arch::x64::{
	rdmsr, wrmsr, EFER_MSR, EFER_SYSCALL_ENABLE, FMASK_MSR, LSTAR_MSR, STAR_MSR, asm_user_copy,
};

mod debug;
use debug::*;
mod process;
use process::*;
mod serial;
use serial::*;

//mod strace;

extern "C" {
    fn syscall_entry();
}

#[derive(Debug)]
#[repr(C)]
pub struct SyscallVals {
    pub options: u32,
	unused: u32,
	pub a1: usize,
	pub a2: usize,
	pub a3: usize,
	pub a4: usize,
	pub a5: usize,
	pub a6: usize,
	pub a7: usize,
	pub a8: usize,
}

impl SyscallVals {
	pub fn get(&self, index: usize) -> Option<usize> {
		match index {
			0 => Some(self.a1),
			1 => Some(self.a2),
			2 => Some(self.a3),
			3 => Some(self.a4),
			4 => Some(self.a5),
			5 => Some(self.a6),
			6 => Some(self.a7),
			7 => Some(self.a8),
			_ => None,
		}
	}
}

macro_rules! syscall_0 {
	($func:expr, $vals:expr) => {
		$func($vals.options)
	};
}

macro_rules! syscall_1 {
	($func:expr, $vals:expr) => {
		$func(
			$vals.options,
			$vals.a1,
		)
	};
}

macro_rules! syscall_2 {
	($func:expr, $vals:expr) => {
		$func(
			$vals.options,
			$vals.a1,
			$vals.a2,
		)
	};
}

macro_rules! syscall_3 {
	($func:expr, $vals:expr) => {
		$func(
			$vals.options,
			$vals.a1,
			$vals.a2,
			$vals.a3,
		)
	};
}

macro_rules! syscall_4 {
	($func:expr, $vals:expr) => {
		$func(
			$vals.options,
			$vals.a1,
			$vals.a2,
			$vals.a3,
			$vals.a4,
		)
	};
}

macro_rules! syscall_5 {
	($func:expr, $vals:expr) => {
		$func(
			$vals.options,
			$vals.a1,
			$vals.a2,
			$vals.a3,
			$vals.a4,
			$vals.a5,
		)
	};
}

macro_rules! syscall_6 {
	($func:expr, $vals:expr) => {
		$func(
			$vals.options,
			$vals.a1,
			$vals.a2,
			$vals.a3,
			$vals.a4,
			$vals.a5,
			$vals.a6,
		)
	};
}

macro_rules! syscall_7 {
	($func:expr, $vals:expr) => {
		$func(
			$vals.options,
			$vals.a1,
			$vals.a2,
			$vals.a3,
			$vals.a4,
			$vals.a5,
			$vals.a6,
			$vals.a7,
		)
	};
}

macro_rules! syscall_8 {
	($func:expr, $vals:expr) => {
		$func(
			$vals.options,
			$vals.a1,
			$vals.a2,
			$vals.a3,
			$vals.a4,
			$vals.a5,
			$vals.a6,
			$vals.a7,
			$vals.a8,
		)
	};
}

macro_rules! sysret_0 {
	($ret:expr, $vals:expr) => {
		match $ret {
			Ok(()) => $vals.a1 = sys::SysErr::Ok.num(),
			Err(err) => $vals.a1 = err.num(),
		}
	};
}

macro_rules! sysret_1 {
	($ret:expr, $vals:expr) => {
		match $ret {
			Ok(n1) => {
				$vals.a1 = sys::SysErr::Ok.num();
				$vals.a2 = n1;
			},
			Err(err) => $vals.a1 = err.num(),
		}
	};
}

macro_rules! sysret_2 {
	($ret:expr, $vals:expr) => {
		match $ret {
			Ok((n1, n2)) => {
				$vals.a1 = sys::SysErr::Ok.num();
				$vals.a2 = n1;
				$vals.a3 = n2;
			},
			Err(err) => $vals.a1 = err.num(),
		}
	};
}

macro_rules! sysret_3 {
	($ret:expr, $vals:expr) => {
		match $ret {
			Ok((n1, n2, n3)) => {
				$vals.a1 = sys::SysErr::Ok.num();
				$vals.a2 = n1;
				$vals.a3 = n2;
				$vals.a4 = n3;
			},
			Err(err) => $vals.a1 = err.num(),
		}
	};
}

macro_rules! sysret_4 {
	($ret:expr, $vals:expr) => {
		match $ret {
			Ok((n1, n2, n3, n4)) => {
				$vals.a1 = sys::SysErr::Ok.num();
				$vals.a2 = n1;
				$vals.a3 = n2;
				$vals.a4 = n3;
				$vals.a5 = n4;
			},
			Err(err) => $vals.a1 = err.num(),
		}
	};
}

macro_rules! sysret_5 {
	($ret:expr, $vals:expr) => {
		match $ret {
			Ok((n1, n2, n3, n4, n5)) => {
				$vals.a1 = sys::SysErr::Ok.num();
				$vals.a2 = n1;
				$vals.a3 = n2;
				$vals.a4 = n3;
				$vals.a5 = n4;
				$vals.a6 = n5;
			},
			Err(err) => $vals.a1 = err.num(),
		}
	};
}

/// This function is called by the assembly syscall entry point
#[no_mangle]
extern "C" fn rust_syscall_entry(syscall_num: u32, vals: &mut SyscallVals) {
	// let strace_args_string = if syscall_num != PRINT_DEBUG {
	// 	Some(strace::get_strace_args_string(syscall_num, vals))
	// } else {
	// 	None
	// };

    match syscall_num {
		// chall calls
		PRINT_DEBUG => sysret_0!(syscall_8!(print_debug, vals), vals),
		// TODO:
		// write to serial with memory
		// read from serial
		// map memory
		// unmap memory
		// spawn (exec elf in new process)
		// setuid (for root)
		// scuffed fs
		// send ipc message
		// recv ipc message
		SERIAL_READ => sysret_0!(syscall_2!(serial_read, vals), vals),
		SERIAL_WRITE => sysret_0!(syscall_2!(serial_read, vals), vals),
		PROCESS_SPAWN => sysret_1!(syscall_2!(process_spawn, vals), vals),
		PROCESS_SEND_MESSAGE => sysret_0!(syscall_3!(process_send_message, vals), vals),
		PROCESS_RECV_MESSAGE => sysret_1!(syscall_2!(process_recv_message, vals), vals),
		PROCESS_SET_UID => sysret_0!(syscall_1!(process_set_uid, vals), vals),
		PROCESS_MAP_MEM => sysret_0!(syscall_2!(process_map_mem, vals), vals),
		PROCESS_UNMAP_MEM => sysret_0!(syscall_1!(process_unmap_mem, vals), vals),
        _ => vals.a1 = SysErr::InvlSyscall.num(),
    }

	// if let Some(args_string) = strace_args_string {
	// 	let ret_string = strace::get_strace_return_string(syscall_num, vals);
	// 	eprintln!("{} -> {}", args_string, ret_string);
	// }
}

fn is_option_set(options: u32, bit: u32) -> bool {
	(options & bit) != 0
}

/// Checks if the weak autodestroy bit is set in the options
fn options_weak_autodestroy(options: u32) -> bool {
	is_option_set(options, 1 << 31)
}

fn copy_from_userspace<T: Pod>(dst: &mut [T], src: *const T) -> KResult<()> {
	let copy_count = dst.len() * size_of::<T>();
	let end_read_addr = (src as usize).checked_add(copy_count)
		.ok_or(SysErr::Overflow)?;

	// forbid reading from kernel memory
	if end_read_addr > *KERNEL_VMA {
		return Err(SysErr::InvlBuffer);
	}

	// safety: it is checked no kernel memory that isn't expecting to be read is read
	// dst is mutable slice to it can be written to
	// reads are valid for T because T is Pod
	let copy_success = unsafe {
		asm_user_copy(dst.as_mut_ptr() as *mut u8, src as *const u8, copy_count)
	};

	if !copy_success {
		Err(SysErr::InvlBuffer)
	} else {
		Ok(())
	}
}

fn copy_vec_from_userspace(src: *const u8, num_bytes: usize) -> KResult<Vec<u8>> {
	// just cap copy amount to be safe
	if num_bytes > 0x1000 {
		Err(SysErr::InvlArgs)
	} else {
		let mut out = alloc::vec![0; num_bytes];
		copy_from_userspace(out.as_mut_slice(), src)?;
		Ok(out)
	}
}

fn copy_to_userspace<T: Pod>(dst: *mut T, src: &[T]) -> KResult<()> {
	let copy_count = src.len() * size_of::<T>();
	let end_write_addr = (dst as usize).checked_add(copy_count)
		.ok_or(SysErr::Overflow)?;

	// forbid writing to kernel memory
	if end_write_addr > *KERNEL_VMA {
		return Err(SysErr::InvlBuffer);
	}

	// safety: it is checked no kernel memory that isn't expecting to be writen to is writen to
	// src is slice so it can be read from
	// reads are valid for T because T is Pod
	let copy_success = unsafe {
		asm_user_copy(dst as *mut u8, src.as_ptr() as *const u8, copy_count)
	};

	if !copy_success {
		Err(SysErr::InvlBuffer)
	} else {
		Ok(())
	}
}

/// Initializes the syscall entry point and enables the syscall instruction
pub fn init() {
    // enable syscall instruction
    let efer = rdmsr(EFER_MSR);
	wrmsr(EFER_MSR, efer | EFER_SYSCALL_ENABLE);

	// tell cpu syscall instruction entry point
	wrmsr(LSTAR_MSR, syscall_entry as usize as u64);

	// tell cpu to disable interrupts on syscall_entry
	wrmsr(FMASK_MSR, 0x200);

	// load correct segment values after syscall and sysret
	wrmsr(STAR_MSR, 0x0013000800000000);
}