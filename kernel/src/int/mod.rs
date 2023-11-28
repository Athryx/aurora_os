use crate::prelude::*;
use crate::sched;
use crate::arch::x64::{cli, hlt, get_cr2};

use userspace_interrupt::{InterruptId, interrupt_manager};

pub mod apic;
pub mod idt;
mod pic;
pub mod pit;
pub mod userspace_interrupt;

// Interrupt vector numbers
pub const EXC_DIVIDE_BY_ZERO: u8 = 0;
pub const EXC_DEBUG: u8 = 1;
pub const EXC_NON_MASK_INTERRUPT: u8 = 2;
pub const EXC_BREAKPOINT: u8 = 3;
pub const EXC_OVERFLOW: u8 = 4;
pub const EXC_BOUND_RANGE_EXCEED: u8 = 5;
pub const EXC_INVALID_OPCODE: u8 = 6;
pub const EXC_DEVICE_UNAVAILABLE: u8 = 7;
pub const EXC_DOUBLE_FAULT: u8 = 8;
pub const EXC_NONE_9: u8 = 9;
pub const EXC_INVALID_TSS: u8 = 10;
pub const EXC_SEGMENT_NOT_PRESENT: u8 = 11;
pub const EXC_STACK_SEGMENT_FULL: u8 = 12;
pub const EXC_GENERAL_PROTECTION_FAULT: u8 = 13;
pub const EXC_PAGE_FAULT: u8 = 14;

pub const PAGE_FAULT_PROTECTION: u64 = 1;
pub const PAGE_FAULT_WRITE: u64 = 1 << 1;
pub const PAGE_FAULT_USER: u64 = 1 << 2;
pub const PAGE_FAULT_RESERVED: u64 = 1 << 3;
pub const PAGE_FAULT_EXECUTE: u64 = 1 << 4;

pub const EXC_NONE_15: u8 = 15;
pub const EXC_X87_FLOATING_POINT: u8 = 16;
pub const EXC_ALIGNMENT_CHECK: u8 = 17;
pub const EXC_MACHINE_CHECK: u8 = 18;
pub const EXC_SIMD_FLOATING_POINT: u8 = 19;
pub const EXC_VIRTUALIZATION: u8 = 20;
pub const EXC_NONE_21: u8 = 21;
pub const EXC_NONE_22: u8 = 22;
pub const EXC_NONE_23: u8 = 23;
pub const EXC_NONE_24: u8 = 24;
pub const EXC_NONE_25: u8 = 25;
pub const EXC_NONE_26: u8 = 26;
pub const EXC_NONE_27: u8 = 27;
pub const EXC_NONE_28: u8 = 28;
pub const EXC_NONE_29: u8 = 29;
pub const EXC_SECURITY: u8 = 30;
pub const EXC_NONE_31: u8 = 31;

// Interrupts 32..40 are used to remap the pic
// even though the pic is disabled it could generate spurious interrupts, so these interrupts are not used
pub const PIC_DISABLE_OFFSET: u8 = 32;

pub const IRQ_APIC_TIMER: u8 = 40;

// TODO: remove this interrupt type
pub const IPI_PROCESS_EXIT: u8 = 41;
pub const IPI_PANIC: u8 = 42;

// The irq src for the pit
pub const PIT_IRQ_SRC: u8 = 0;
// This interrupt is used by pit to calibrate local apic timer
pub const PIT_TICK: u8 = 43;

// This is where spurious interrupts are sent to, no one listens
// NOTE: on some processors, according to intel manuals, bits 0-3 of the spurious vector register are always 1,
// so we should always choose a spurious vector number with bits 0-3 having 1
// 47 = 0x2f
pub const SPURIOUS: u8 = 47;

// Userspace handleable interrupts will use the interrupt
// number starting at this interrupt all the way to end of the idt
pub const USER_INTERRUPT_START: u8 = 48;
pub const USER_INTERRUPT_COUNT: usize = 256 - USER_INTERRUPT_START as usize;


#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct Registers {
    pub rax: usize,
    pub rbx: usize,
    pub rcx: usize,
    pub rdx: usize,
    pub rbp: usize,
    pub rsp: usize,
    pub rdi: usize,
    pub rsi: usize,
    pub r8: usize,
    pub r9: usize,
    pub r10: usize,
    pub r11: usize,
    pub r12: usize,
    pub r13: usize,
    pub r14: usize,
    pub r15: usize,
    pub rflags: usize,
    pub rip: usize,
    pub cs: u16,
    pub ss: u16,
}

fn double_fault(registers: &Registers) {
    panic!("double fault\nregisters:\n{:x?}", registers);
}

fn gp_exception(registers: &Registers) {
    panic!("general protection exception\nregisters:\n{:x?}", registers);
}

fn page_fault(registers: &Registers, error_code: u64) {
    let ring = if error_code & PAGE_FAULT_USER != 0 {
		"user"
	} else {
		"kernel"
	};

	let action = if error_code & PAGE_FAULT_EXECUTE != 0 {
		"instruction fetch"
	} else if error_code & PAGE_FAULT_WRITE != 0 {
		"write"
	} else {
		"read"
	};

	// can't indent because it will print tabs
	panic!(
		r"page fault accessing virtual address {:x}
page fault during {} {}
non present page: {}
reserved bit set: {}
registers:
{:x?}",
		get_cr2(),
		ring,
		action,
		error_code & PAGE_FAULT_PROTECTION == 0,
		error_code & PAGE_FAULT_RESERVED != 0,
		registers
	);
}

/// This function runs if a nother cpu panics, just halt the currnet cpu
fn ipi_panic() {
    loop {
        cli();
        hlt();
    }
}

/// Called by each assembly interrupt handler
#[no_mangle]
extern "C" fn rust_int_handler(int_num: u8, registers: &Registers, error_code: u64) {
    match int_num {
        EXC_DOUBLE_FAULT => double_fault(registers),
        EXC_GENERAL_PROTECTION_FAULT => gp_exception(registers),
        EXC_PAGE_FAULT => page_fault(registers, error_code),
        // do not send eoi here because this is only ever used for oneshot timer
        PIT_TICK => pit::PIT.irq_handler(),
        IRQ_APIC_TIMER => {
            cpu_local_data().local_apic().tick();
            sched::timer_handler();
            cpu_local_data().local_apic().eoi();
        },
        IPI_PROCESS_EXIT => sched::exit_handler(),
        IPI_PANIC => ipi_panic(),
        _ if int_num >= USER_INTERRUPT_START => {
            let interrupt_id = InterruptId {
                cpu: prid(),
                interrupt_num: int_num,
            };

            // FIXME: figure out what to do if this fails
            let _ = interrupt_manager().notify_interrupt(interrupt_id);

            // TODO: figure out if sending eoi is necessary for msi interrupts
            // (because for now this is what all these user interrupts are intended for)
        },
        _ => (),
    }
}