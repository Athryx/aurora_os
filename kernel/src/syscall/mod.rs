use sys::syscall_nums::*;

use crate::prelude::*;
use crate::arch::x64::{
	rdmsr, wrmsr, EFER_MSR, EFER_SYSCALL_ENABLE, FMASK_MSR, LSTAR_MSR, STAR_MSR,
};

mod debug;
use debug::*;
mod key;
use key::*;
mod memory;
use memory::*;
mod process;
use process::*;
mod spawner;
use spawner::*;

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
    match syscall_num {
		PRINT_DEBUG => sysret_0!(print_debug(
			vals.options,
			vals.a1,
			vals.a2,
			vals.a3,
			vals.a4,
			vals.a5,
			vals.a6,
			vals.a7,
			vals.a8,
		), vals),
		PROCESS_NEW => sysret_1!(syscall_2!(process_new, vals), vals),
		PROCESS_EXIT => sysret_0!(syscall_1!(process_exit, vals), vals),
		THREAD_NEW => sysret_1!(syscall_7!(thread_new, vals), vals),
		THREAD_YIELD => sysret_0!(thread_yield(), vals),
		THREAD_SUSPEND => sysret_0!(syscall_1!(thread_suspend, vals), vals),
		MEMORY_MAP => sysret_1!(syscall_3!(memory_map, vals), vals),
		MEMORY_UNMAP => sysret_0!(syscall_2!(memory_unmap, vals), vals),
		MEMORY_NEW => sysret_1!(syscall_2!(memory_new, vals), vals),
		KEY_NEW => sysret_1!(syscall_1!(key_new, vals), vals),
		KEY_ID => sysret_1!(syscall_1!(key_id, vals), vals),
		SPAWNER_NEW => sysret_1!(syscall_2!(spawner_new, vals), vals),
		SPAWNER_KILL_ALL => sysret_0!(syscall_1!(spawner_kill_all, vals), vals),
        _ => vals.a1 = SysErr::InvlSyscall.num(),
    }
}

/// Checks if the weak autodestroy bit is set in the options
fn options_weak_autodestroy(options: u32) -> bool {
	options & (1 << 31) != 0
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