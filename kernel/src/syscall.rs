use crate::prelude::*;
use crate::arch::x64::{
	rdmsr, wrmsr, EFER_MSR, EFER_SYSCALL_ENABLE, FMASK_MSR, LSTAR_MSR, STAR_MSR,
};

extern "C" {
    fn syscall_entry();
}

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
	pub a9: usize,
	pub a10: usize,
	pub rsp: usize,
	pub rflags: usize,
    pub rip: usize,
}

/// This function is called by the assembly syscall entry point
#[no_mangle]
extern "C" fn rust_syscall_entry(syscall_num: u32, syscall_vals: &mut SyscallVals) {
    match syscall_num {
        _ => syscall_vals.a1 = SysErr::InvlSyscall.num(),
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