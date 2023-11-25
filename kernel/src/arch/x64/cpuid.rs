use core::arch::asm;

use crate::prelude::*;

#[derive(Debug, Clone, Copy)]
struct CpuidRet {
    eax: u32,
    ebx: u32,
    ecx: u32,
    edx: u32,
}

fn cpuid(n: u32) -> CpuidRet {
    let eax: u32;
    let ebx: u32;
    let ecx: u32;
    let edx: u32;
    unsafe {
        asm!("push rbx", 
			 "cpuid",
			 "mov edi, ebx", 
			 "pop rbx",
			 inout("eax") n => eax,
			 out("edi") ebx,
			 out("ecx") ecx,
			 out("edx") edx,
			 options(nomem, nostack));
    }

    CpuidRet {
        eax,
        ebx,
        ecx,
        edx,
    }
}

pub fn has_apic() -> bool {
    get_bits(cpuid(1).edx as usize, 9..10) == 1
}

pub fn apic_id() -> u8 {
    get_bits(cpuid(1).ebx as usize, 24..32) as u8
}

pub fn core_clock_freq() -> u32 {
    cpuid(0x15).ecx
}

/// Checks for presence of page attribute table, which allows setting all cache control modes with just page table entries
pub fn has_pat() -> bool {
    get_bits(cpuid(1).edx as usize, 16..17) == 1
}