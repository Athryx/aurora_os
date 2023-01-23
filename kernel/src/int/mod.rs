use crate::{sched::Registers, prelude::cpu_local_data};

pub mod apic;
pub mod idt;
mod pic;
pub mod pit;

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

pub const IRQ_BASE: u8 = 32;

pub const IRQ_PIT_TIMER: u8 = IRQ_BASE;
pub const IRQ_KEYBOARD: u8 = IRQ_BASE + 1;
pub const IRQ_SERIAL_PORT_2: u8 = IRQ_BASE + 3;
pub const IRQ_SERIAL_PORT_1: u8 = IRQ_BASE + 4;
pub const IRQ_PARALLEL_PORT_2_3: u8 = IRQ_BASE + 5;
pub const IRQ_FLOPPY_DISK: u8 = IRQ_BASE + 6;
pub const IRQ_PARALLEL_PORT_1: u8 = IRQ_BASE + 7;

pub const IRQ_CLOCK: u8 = IRQ_BASE + 8;
pub const IRQ_ACPI: u8 = IRQ_BASE + 9;
pub const IRQ_NONE_1: u8 = IRQ_BASE + 10;
pub const IRQ_NONE_2: u8 = IRQ_BASE + 11;
pub const IRQ_MOUSE: u8 = IRQ_BASE + 12;
pub const IRQ_CO_PROCESSOR: u8 = IRQ_BASE + 13;
pub const IRQ_PRIMARY_ATA: u8 = IRQ_BASE + 14;
pub const IRQ_SECONDARY_ATA: u8 = IRQ_BASE + 15;

pub const IRQ_APIC_TIMER: u8 = 48;

pub const INT_SCHED: u8 = 128;

pub const IPI_PROCESS_EXIT: u8 = 129;
pub const IPI_PANIC: u8 = 130;

// This is where spurious interrupts are sent to, no one listens
// NOTE: on some processors, according to intel manuals, bits 0-3 of the spurious vector register are always 0,
// so we should always choose a spurious vector number with bits 0-3 zeroed
pub const SPURIOUS: u8 = 0xf0;


/// Called by each assembly interrupt handler
/// 
/// Returns true to indicate if registers have changed and should be reloaded
#[no_mangle]
extern "C" fn rust_int_handler(int_num: u8, regs: &mut Registers, error_code: u64) -> bool {
    match int_num {
        IRQ_PIT_TIMER => {
            pit::PIT.irq_handler();
            false
        },
        IRQ_APIC_TIMER => {
            cpu_local_data().local_apic().tick();
            false
        },
        _ => false,
    }
}

/// Called by assembly code to indicate end of interrupt handler
#[no_mangle]
extern "C" fn eoi() {
    cpu_local_data().local_apic().eoi();
}
