use sys::syscall_nums::*;

use crate::alloc::root_alloc_ref;
use crate::prelude::*;
use crate::arch::x64::{
	rdmsr, wrmsr, EFER_MSR, EFER_SYSCALL_ENABLE, FMASK_MSR, LSTAR_MSR, STAR_MSR,
};

mod cap;
use cap::*;
mod channel;
use channel::*;
mod debug;
use debug::*;
mod event;
mod drop_check;
use drop_check::*;
mod event_pool;
use event_pool::*;
mod interrupt;
use interrupt::*;
mod key;
use key::*;
mod memory;
use memory::*;
mod mmio;
use mmio::*;
mod thread;
use thread::*;
mod thread_group;
use thread_group::*;

mod strace;

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
	let strace_args_string = if syscall_num != PRINT_DEBUG {
		Some(strace::get_strace_args_string(syscall_num, vals))
	} else {
		None
	};

    match syscall_num {
		PRINT_DEBUG => sysret_0!(syscall_8!(print_debug, vals), vals),
		THREAD_GROUP_NEW => sysret_1!(syscall_2!(thread_group_new, vals), vals),
		THREAD_GROUP_EXIT => sysret_0!(syscall_1!(thread_group_exit, vals), vals),
		THREAD_NEW => sysret_2!(syscall_6!(thread_new, vals), vals),
		THREAD_YIELD => sysret_0!(thread_yield(), vals),
		THREAD_DESTROY => sysret_0!(syscall_1!(thread_destroy, vals), vals),
		THREAD_SUSPEND => sysret_0!(syscall_1!(thread_suspend, vals), vals),
		THREAD_RESUME => sysret_0!(syscall_1!(thread_resume, vals), vals),
		THREAD_SET_PROPERTY => sysret_0!(syscall_2!(thread_set_property, vals), vals),
		THREAD_HANDLE_THREAD_EXIT_SYNC => sysret_0!(syscall_2!(thread_handle_thread_exit_sync, vals), vals),
		THREAD_HANDLE_THREAD_EXIT_ASYNC => sysret_0!(syscall_3!(thread_handle_thread_exit_async, vals), vals),
		CAP_CLONE => sysret_1!(syscall_3!(cap_clone, vals), vals),
		CAP_DESTROY => sysret_0!(syscall_2!(cap_destroy, vals), vals),
		ADDRESS_SPACE_NEW => sysret_1!(syscall_1!(address_space_new, vals), vals),
		ADDRESS_SPACE_UNMAP => sysret_0!(syscall_2!(address_space_unmap, vals), vals),
		MEMORY_MAP => sysret_1!(syscall_5!(memory_map, vals), vals),
		MEMORY_UPDATE_MAPPING => sysret_1!(syscall_3!(memory_update_mapping, vals), vals),
		MEMORY_NEW => sysret_2!(syscall_2!(memory_new, vals), vals),
		MEMORY_GET_SIZE => sysret_1!(syscall_1!(memory_get_size, vals), vals),
		MEMORY_RESIZE => sysret_1!(syscall_2!(memory_resize, vals), vals),
		EVENT_POOL_NEW => sysret_1!(syscall_2!(event_pool_new, vals), vals),
		EVENT_POOL_MAP => sysret_1!(syscall_3!(event_pool_map, vals), vals),
		EVENT_POOL_AWAIT => sysret_2!(syscall_2!(event_pool_await, vals), vals),
		CHANNEL_NEW => sysret_1!(syscall_1!(channel_new, vals), vals),
		CHANNEL_TRY_SEND => sysret_1!(syscall_4!(channel_try_send, vals), vals),
		CHANNEL_SYNC_SEND => sysret_1!(syscall_5!(channel_sync_send, vals), vals),
		CHANNEL_ASYNC_SEND => sysret_0!(syscall_6!(channel_async_send, vals), vals),
		CHANNEL_TRY_RECV => sysret_2!(syscall_4!(channel_try_recv, vals), vals),
		CHANNEL_SYNC_RECV => sysret_2!(syscall_5!(channel_sync_recv, vals), vals),
		CHANNEL_ASYNC_RECV => sysret_0!(syscall_3!(channel_async_recv, vals), vals),
		CHANNEL_SYNC_CALL => sysret_1!(syscall_8!(channel_sync_call, vals), vals),
		CHANNEL_ASYNC_CALL => sysret_0!(syscall_6!(channel_async_call, vals), vals),
		REPLY_REPLY => sysret_1!(syscall_4!(reply_reply, vals), vals),
		KEY_NEW => sysret_1!(syscall_1!(key_new, vals), vals),
		KEY_ID => sysret_1!(syscall_1!(key_id, vals), vals),
		DROP_CHECK_NEW => sysret_2!(syscall_2!(drop_check_new, vals), vals),
		DROP_CHECK_RECIEVER_HANDLE_CAP_DROP_SYNC => sysret_1!(syscall_2!(drop_check_reciever_handle_cap_drop_sync, vals), vals),
		DROP_CHECK_RECIEVER_HANDLE_CAP_DROP_ASYNC => sysret_0!(syscall_3!(drop_check_reciever_handle_cap_drop_async, vals), vals),
		MMIO_ALLOCATOR_ALLOC => sysret_1!(syscall_4!(mmio_allocator_alloc, vals), vals),
		PHYS_MEM_MAP => sysret_1!(syscall_3!(phys_mem_map, vals), vals),
		PHYS_MEM_GET_SIZE => sysret_1!(syscall_1!(phys_mem_get_size, vals), vals),
		INTERRUPT_NEW => sysret_3!(syscall_2!(interrupt_new, vals), vals),
		INTERRUPT_ID => sysret_2!(syscall_1!(interrupt_id, vals), vals),
		INTERRUPT_HANDLE_INTERRUPT_TRIGGER_SYNC => sysret_0!(syscall_2!(interrupt_handle_interrupt_trigger_sync, vals), vals),
		INTERRUPT_HANDLE_INTERRUPT_TRIGGER_ASYNC => sysret_0!(syscall_3!(interrupt_handle_interrupt_trigger_async, vals), vals),
        _ => vals.a1 = SysErr::InvlSyscall.num(),
    }

	if let Some(args_string) = strace_args_string {
		let ret_string = strace::get_strace_return_string(syscall_num, vals);
		eprintln!("{} -> {}", args_string, ret_string);
	}
}

fn is_option_set(options: u32, bit: u32) -> bool {
	(options & bit) != 0
}

/// Checks if the weak autodestroy bit is set in the options
fn options_weak_autodestroy(options: u32) -> bool {
	is_option_set(options, 1 << 31)
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