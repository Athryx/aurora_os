use core::arch::asm;

use crate::arch::x64::CPUPrivLevel;
use crate::prelude::*;

#[derive(Debug, Clone, Copy)]
enum IntHandlerType {
    /// An interrupt handler which returns to the next instruction after being invoked
    Interrupt,
    /// An interrupt handler which returns to the same instruction after being invoked
    Trap,
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
struct IdtEntry {
    addr1: u16,
    // must be kernel code selector
    code_selector: u16,
    // if non zero, will set the stack to stack determined by indexing ist in the tss when interupt occors
    ist: u8,
    attr: u8,
    addr2: u16,
    addr3: u32,
    zero: u32,
}

impl IdtEntry {
    /// Ring is the privallage level that is allowed to call this interrupt handler
    fn new(addr: usize, handler_type: IntHandlerType, ring: CPUPrivLevel) -> Self {
        IdtEntry {
            addr1: get_bits(addr, 0..16) as _,
            addr2: get_bits(addr, 16..32) as _,
            addr3: get_bits(addr, 32..64) as _,
            code_selector: 8,
            ist: 0,
            attr: match handler_type {
                IntHandlerType::Interrupt => 0x80 | ring.n() << 5 | 0xe,
                IntHandlerType::Trap => 0x80 | ring.n() << 5 | 0xf,
            },
            zero: 0,
        }
    }

    fn none() -> Self {
        IdtEntry {
            addr1: 0,
            addr2: 0,
            addr3: 0,
            code_selector: 0,
            ist: 0,
            attr: 0,
            zero: 0,
        }
    }
}

macro_rules! make_idt_entry {
    ($idt:expr, $num:literal, $handler_type:expr, $priv_level:expr) => {
        concat_idents::concat_idents!(fn_name = int_handler_, $num {
            extern "C" {
                fn fn_name();
            }
            $idt.entries[$num] = IdtEntry::new(fn_name as usize, $handler_type, $priv_level);
        })
    };
}

macro_rules! make_idt_entry_r0 {
    ($idt:expr, $num:literal) => {
        make_idt_entry!($idt, $num, IntHandlerType::Interrupt, CPUPrivLevel::Ring0)
    };
}

/// Passed to lidt instruction to load idt
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
struct IdtPointer {
    limit: u16,
    base: u64,
}

#[derive(Debug)]
pub struct Idt {
    // idt entries in the idt given to the cpu
    entries: [IdtEntry; Self::NUM_ENTRIES],
}

impl Idt {
    const NUM_ENTRIES: usize = 256;

    pub fn new() -> Self {
        let mut out = Idt {
            entries: [IdtEntry::none(); Self::NUM_ENTRIES],
        };

        // CPU exception interrupt vectors
        make_idt_entry_r0!(out, 0);
        make_idt_entry_r0!(out, 1);
        make_idt_entry_r0!(out, 2);
        make_idt_entry_r0!(out, 3);
        make_idt_entry_r0!(out, 4);
        make_idt_entry_r0!(out, 5);
        make_idt_entry_r0!(out, 6);
        make_idt_entry_r0!(out, 7);
        make_idt_entry_r0!(out, 8);
        make_idt_entry_r0!(out, 9);
        make_idt_entry_r0!(out, 10);
        make_idt_entry_r0!(out, 11);
        make_idt_entry_r0!(out, 12);
        make_idt_entry_r0!(out, 13);
        make_idt_entry_r0!(out, 14);
        make_idt_entry_r0!(out, 15);
        make_idt_entry_r0!(out, 16);
        make_idt_entry_r0!(out, 17);
        make_idt_entry_r0!(out, 18);
        make_idt_entry_r0!(out, 19);
        make_idt_entry_r0!(out, 20);
        make_idt_entry_r0!(out, 21);
        make_idt_entry_r0!(out, 22);
        make_idt_entry_r0!(out, 23);
        make_idt_entry_r0!(out, 24);
        make_idt_entry_r0!(out, 25);
        make_idt_entry_r0!(out, 26);
        make_idt_entry_r0!(out, 27);
        make_idt_entry_r0!(out, 28);
        make_idt_entry_r0!(out, 29);
        make_idt_entry_r0!(out, 30);
        make_idt_entry_r0!(out, 31);

        // PIC interrupt vectors
        make_idt_entry_r0!(out, 32);
        make_idt_entry_r0!(out, 33);
        make_idt_entry_r0!(out, 34);
        make_idt_entry_r0!(out, 35);
        make_idt_entry_r0!(out, 36);
        make_idt_entry_r0!(out, 37);
        make_idt_entry_r0!(out, 38);
        make_idt_entry_r0!(out, 39);
        make_idt_entry_r0!(out, 40);
        make_idt_entry_r0!(out, 41);
        make_idt_entry_r0!(out, 42);
        make_idt_entry_r0!(out, 43);
        make_idt_entry_r0!(out, 44);
        make_idt_entry_r0!(out, 45);
        make_idt_entry_r0!(out, 46);
        make_idt_entry_r0!(out, 47);

        // Used for scheduler to switch threads
        make_idt_entry_r0!(out, 128);

        out.load();
        out
    }

    fn load(&self) {
        let idt_pointer = IdtPointer {
            limit: (size_of::<[IdtEntry; Self::NUM_ENTRIES]>() - 1) as _,
            base: &self.entries as *const _ as _,
        };

        unsafe {
            asm!("lidt [{}]", in(reg) &idt_pointer, options(nostack));
        }
    }
}
