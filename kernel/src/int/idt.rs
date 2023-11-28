use core::arch::asm;

use paste::paste;

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
        paste! {
            extern "C" {
                fn [<int_handler_ $num>]();
            }
            $idt.entries[$num] = IdtEntry::new([<int_handler_ $num>] as usize, $handler_type, $priv_level);
        }
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
        make_idt_entry_r0!(out, 48);
        make_idt_entry_r0!(out, 49);
        make_idt_entry_r0!(out, 50);
        make_idt_entry_r0!(out, 51);
        make_idt_entry_r0!(out, 52);
        make_idt_entry_r0!(out, 53);
        make_idt_entry_r0!(out, 54);
        make_idt_entry_r0!(out, 55);
        make_idt_entry_r0!(out, 56);
        make_idt_entry_r0!(out, 57);
        make_idt_entry_r0!(out, 58);
        make_idt_entry_r0!(out, 59);
        make_idt_entry_r0!(out, 60);
        make_idt_entry_r0!(out, 61);
        make_idt_entry_r0!(out, 62);
        make_idt_entry_r0!(out, 63);
        make_idt_entry_r0!(out, 64);
        make_idt_entry_r0!(out, 65);
        make_idt_entry_r0!(out, 66);
        make_idt_entry_r0!(out, 67);
        make_idt_entry_r0!(out, 68);
        make_idt_entry_r0!(out, 69);
        make_idt_entry_r0!(out, 70);
        make_idt_entry_r0!(out, 71);
        make_idt_entry_r0!(out, 72);
        make_idt_entry_r0!(out, 73);
        make_idt_entry_r0!(out, 74);
        make_idt_entry_r0!(out, 75);
        make_idt_entry_r0!(out, 76);
        make_idt_entry_r0!(out, 77);
        make_idt_entry_r0!(out, 78);
        make_idt_entry_r0!(out, 79);
        make_idt_entry_r0!(out, 80);
        make_idt_entry_r0!(out, 81);
        make_idt_entry_r0!(out, 82);
        make_idt_entry_r0!(out, 83);
        make_idt_entry_r0!(out, 84);
        make_idt_entry_r0!(out, 85);
        make_idt_entry_r0!(out, 86);
        make_idt_entry_r0!(out, 87);
        make_idt_entry_r0!(out, 88);
        make_idt_entry_r0!(out, 89);
        make_idt_entry_r0!(out, 90);
        make_idt_entry_r0!(out, 91);
        make_idt_entry_r0!(out, 92);
        make_idt_entry_r0!(out, 93);
        make_idt_entry_r0!(out, 94);
        make_idt_entry_r0!(out, 95);
        make_idt_entry_r0!(out, 96);
        make_idt_entry_r0!(out, 97);
        make_idt_entry_r0!(out, 98);
        make_idt_entry_r0!(out, 99);
        make_idt_entry_r0!(out, 100);
        make_idt_entry_r0!(out, 101);
        make_idt_entry_r0!(out, 102);
        make_idt_entry_r0!(out, 103);
        make_idt_entry_r0!(out, 104);
        make_idt_entry_r0!(out, 105);
        make_idt_entry_r0!(out, 106);
        make_idt_entry_r0!(out, 107);
        make_idt_entry_r0!(out, 108);
        make_idt_entry_r0!(out, 109);
        make_idt_entry_r0!(out, 110);
        make_idt_entry_r0!(out, 111);
        make_idt_entry_r0!(out, 112);
        make_idt_entry_r0!(out, 113);
        make_idt_entry_r0!(out, 114);
        make_idt_entry_r0!(out, 115);
        make_idt_entry_r0!(out, 116);
        make_idt_entry_r0!(out, 117);
        make_idt_entry_r0!(out, 118);
        make_idt_entry_r0!(out, 119);
        make_idt_entry_r0!(out, 120);
        make_idt_entry_r0!(out, 121);
        make_idt_entry_r0!(out, 122);
        make_idt_entry_r0!(out, 123);
        make_idt_entry_r0!(out, 124);
        make_idt_entry_r0!(out, 125);
        make_idt_entry_r0!(out, 126);
        make_idt_entry_r0!(out, 127);
        make_idt_entry_r0!(out, 128);
        make_idt_entry_r0!(out, 129);
        make_idt_entry_r0!(out, 130);
        make_idt_entry_r0!(out, 131);
        make_idt_entry_r0!(out, 132);
        make_idt_entry_r0!(out, 133);
        make_idt_entry_r0!(out, 134);
        make_idt_entry_r0!(out, 135);
        make_idt_entry_r0!(out, 136);
        make_idt_entry_r0!(out, 137);
        make_idt_entry_r0!(out, 138);
        make_idt_entry_r0!(out, 139);
        make_idt_entry_r0!(out, 140);
        make_idt_entry_r0!(out, 141);
        make_idt_entry_r0!(out, 142);
        make_idt_entry_r0!(out, 143);
        make_idt_entry_r0!(out, 144);
        make_idt_entry_r0!(out, 145);
        make_idt_entry_r0!(out, 146);
        make_idt_entry_r0!(out, 147);
        make_idt_entry_r0!(out, 148);
        make_idt_entry_r0!(out, 149);
        make_idt_entry_r0!(out, 150);
        make_idt_entry_r0!(out, 151);
        make_idt_entry_r0!(out, 152);
        make_idt_entry_r0!(out, 153);
        make_idt_entry_r0!(out, 154);
        make_idt_entry_r0!(out, 155);
        make_idt_entry_r0!(out, 156);
        make_idt_entry_r0!(out, 157);
        make_idt_entry_r0!(out, 158);
        make_idt_entry_r0!(out, 159);
        make_idt_entry_r0!(out, 160);
        make_idt_entry_r0!(out, 161);
        make_idt_entry_r0!(out, 162);
        make_idt_entry_r0!(out, 163);
        make_idt_entry_r0!(out, 164);
        make_idt_entry_r0!(out, 165);
        make_idt_entry_r0!(out, 166);
        make_idt_entry_r0!(out, 167);
        make_idt_entry_r0!(out, 168);
        make_idt_entry_r0!(out, 169);
        make_idt_entry_r0!(out, 170);
        make_idt_entry_r0!(out, 171);
        make_idt_entry_r0!(out, 172);
        make_idt_entry_r0!(out, 173);
        make_idt_entry_r0!(out, 174);
        make_idt_entry_r0!(out, 175);
        make_idt_entry_r0!(out, 176);
        make_idt_entry_r0!(out, 177);
        make_idt_entry_r0!(out, 178);
        make_idt_entry_r0!(out, 179);
        make_idt_entry_r0!(out, 180);
        make_idt_entry_r0!(out, 181);
        make_idt_entry_r0!(out, 182);
        make_idt_entry_r0!(out, 183);
        make_idt_entry_r0!(out, 184);
        make_idt_entry_r0!(out, 185);
        make_idt_entry_r0!(out, 186);
        make_idt_entry_r0!(out, 187);
        make_idt_entry_r0!(out, 188);
        make_idt_entry_r0!(out, 189);
        make_idt_entry_r0!(out, 190);
        make_idt_entry_r0!(out, 191);
        make_idt_entry_r0!(out, 192);
        make_idt_entry_r0!(out, 193);
        make_idt_entry_r0!(out, 194);
        make_idt_entry_r0!(out, 195);
        make_idt_entry_r0!(out, 196);
        make_idt_entry_r0!(out, 197);
        make_idt_entry_r0!(out, 198);
        make_idt_entry_r0!(out, 199);
        make_idt_entry_r0!(out, 200);
        make_idt_entry_r0!(out, 201);
        make_idt_entry_r0!(out, 202);
        make_idt_entry_r0!(out, 203);
        make_idt_entry_r0!(out, 204);
        make_idt_entry_r0!(out, 205);
        make_idt_entry_r0!(out, 206);
        make_idt_entry_r0!(out, 207);
        make_idt_entry_r0!(out, 208);
        make_idt_entry_r0!(out, 209);
        make_idt_entry_r0!(out, 210);
        make_idt_entry_r0!(out, 211);
        make_idt_entry_r0!(out, 212);
        make_idt_entry_r0!(out, 213);
        make_idt_entry_r0!(out, 214);
        make_idt_entry_r0!(out, 215);
        make_idt_entry_r0!(out, 216);
        make_idt_entry_r0!(out, 217);
        make_idt_entry_r0!(out, 218);
        make_idt_entry_r0!(out, 219);
        make_idt_entry_r0!(out, 220);
        make_idt_entry_r0!(out, 221);
        make_idt_entry_r0!(out, 222);
        make_idt_entry_r0!(out, 223);
        make_idt_entry_r0!(out, 224);
        make_idt_entry_r0!(out, 225);
        make_idt_entry_r0!(out, 226);
        make_idt_entry_r0!(out, 227);
        make_idt_entry_r0!(out, 228);
        make_idt_entry_r0!(out, 229);
        make_idt_entry_r0!(out, 230);
        make_idt_entry_r0!(out, 231);
        make_idt_entry_r0!(out, 232);
        make_idt_entry_r0!(out, 233);
        make_idt_entry_r0!(out, 234);
        make_idt_entry_r0!(out, 235);
        make_idt_entry_r0!(out, 236);
        make_idt_entry_r0!(out, 237);
        make_idt_entry_r0!(out, 238);
        make_idt_entry_r0!(out, 239);
        make_idt_entry_r0!(out, 240);
        make_idt_entry_r0!(out, 241);
        make_idt_entry_r0!(out, 242);
        make_idt_entry_r0!(out, 243);
        make_idt_entry_r0!(out, 244);
        make_idt_entry_r0!(out, 245);
        make_idt_entry_r0!(out, 246);
        make_idt_entry_r0!(out, 247);
        make_idt_entry_r0!(out, 248);
        make_idt_entry_r0!(out, 249);
        make_idt_entry_r0!(out, 250);
        make_idt_entry_r0!(out, 251);
        make_idt_entry_r0!(out, 252);
        make_idt_entry_r0!(out, 253);
        make_idt_entry_r0!(out, 254);
        make_idt_entry_r0!(out, 255);

        out
    }

    pub fn load(&self) {
        let idt_pointer = IdtPointer {
            limit: (size_of::<[IdtEntry; Self::NUM_ENTRIES]>() - 1) as _,
            base: &self.entries as *const _ as _,
        };

        unsafe {
            asm!("lidt [{}]", in(reg) &idt_pointer, options(nostack));
        }
    }
}
