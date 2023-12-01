use core::arch::asm;
use core::time::Duration;

use crate::prelude::*;
use sys::MemoryCacheSetting;

use self::cpuid::has_pat;

pub mod cpuid;

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CPUPrivLevel {
    Ring0 = 0,
    Ring3 = 3,
}

impl CPUPrivLevel {
    pub const fn n(&self) -> u8 {
        *self as u8
    }

    pub const fn get_cs(&self) -> u16 {
        match self {
            Self::Ring0 => 0x8,
            Self::Ring3 => 0x23,
        }
    }

    pub const fn get_ds(&self) -> u16 {
        match self {
            Self::Ring0 => 0x10,
            Self::Ring3 => 0x1b,
        }
    }

    pub fn is_ring0(&self) -> bool {
        *self == Self::Ring0
    }

    pub fn is_ring3(&self) -> bool {
        *self == Self::Ring3
    }
}

// bochs magic breakpoint
#[inline]
pub fn bochs_break() {
    unsafe { asm!("xchg bx, bx", options(nomem, nostack)) }
}

pub const PAT_MSR: u32 = 0x277;

#[repr(u8)]
#[derive(Debug, Clone, Copy)]
pub enum PatEntry {
    StrongUncacheable = 0,
    WriteCombining = 1,
    WriteThrough = 4,
    WriteProtected = 5,
    /// Default cacheing which should be used for regular memory
    WriteBack = 6,
    /// This version of uncacheable can be overriden by mttrs
    Uncacheable = 7,
}

/// These bits will be used as an index into the page attribute table to determine cache control for a given region of memory
#[derive(Debug, Clone, Copy)]
pub struct PageTableCacheBits {
    pub pwt: bool,
    pub pcd: bool,
    pub pat: bool,
}

impl PatEntry {
    pub fn to_page_table_bits(self) -> PageTableCacheBits {
        match self {
            Self::StrongUncacheable => PageTableCacheBits {
                pwt: true,
                pcd: true,
                pat: false,
            },
            Self::WriteCombining => PageTableCacheBits {
                pwt: false,
                pcd: true,
                pat: false,
            },
            Self::WriteThrough => PageTableCacheBits {
                pwt: true,
                pcd: false,
                pat: false,
            },
            Self::WriteProtected => panic!("write protected memory type currently not supported"),
            Self::WriteBack => PageTableCacheBits {
                pwt: false,
                pcd: false,
                pat: false,
            },
            Self::Uncacheable => PageTableCacheBits {
                pwt: false,
                pcd: false,
                pat: true,
            },
        }
    }
}

impl From<MemoryCacheSetting> for PatEntry {
    fn from(value: MemoryCacheSetting) -> Self {
        match value {
            MemoryCacheSetting::WriteBack => Self::WriteBack,
            MemoryCacheSetting::WriteThrough => Self::WriteThrough,
            MemoryCacheSetting::WriteConbining => Self::WriteCombining,
            MemoryCacheSetting::Uncached => Self::StrongUncacheable,
        }
    }
}

fn init_pat() {
    if !has_pat() {
        panic!("cpu without page attribute table (pat) not supported");
    }

    let pat_value = PatEntry::WriteBack as u64
        | (PatEntry::WriteThrough as u64) << 8
        | (PatEntry::WriteCombining as u64) << 16
        | (PatEntry::StrongUncacheable as u64) << 24
        | (PatEntry::Uncacheable as u64) << 32
        | (PatEntry::Uncacheable as u64) << 40
        | (PatEntry::Uncacheable as u64) << 48
        | (PatEntry::Uncacheable as u64) << 56;
    
    wrmsr(PAT_MSR, pat_value);
}

pub const EFER_MSR: u32 = 0xc0000080;
// TODO: use bitflags
pub const EFER_EXEC_DISABLE: u64 = 1 << 11;
pub const EFER_SYSCALL_ENABLE: u64 = 1;

pub const FSBASE_MSR: u32 = 0xc0000100;
pub const GSBASE_MSR: u32 = 0xc0000101;
pub const GSBASEK_MSR: u32 = 0xc0000102;

pub const STAR_MSR: u32 = 0xc0000081;
pub const LSTAR_MSR: u32 = 0xc0000082;
pub const RTAR_MSR: u32 = 0xc0000083;
pub const FMASK_MSR: u32 = 0xc0000084;

#[inline]
pub fn rdmsr(msr: u32) -> u64 {
    let low: u32;
    let high: u32;
    unsafe {
        asm!("rdmsr", in("ecx") msr, out("eax") low, out("edx") high, options(nomem, nostack));
    }
    ((high as u64) << 32) | low as u64
}

#[inline]
pub fn wrmsr(msr: u32, data: u64) {
    let low = get_bits(data as usize, 0..32);
    let high = get_bits(data as usize, 32..64);
    unsafe {
        asm!("wrmsr", in("ecx") msr, in("eax") low, in("edx") high, options(nomem, nostack));
    }
}

#[inline]
pub fn hlt() {
    unsafe {
        asm!("hlt", options(nomem, nostack));
    }
}

// TODO: use bitflags
pub const RFLAGS_INT: usize = 1 << 9;

#[inline]
pub fn get_flags() -> usize {
    let out;
    unsafe {
        asm!("pushfq\npop {}", out(reg) out, options(nomem));
    }
    out
}

#[inline]
pub fn set_flags(flags: usize) {
    unsafe {
        asm!("push {}\npopfq", in(reg) flags);
    }
}

#[inline]
pub fn cli() {
    unsafe {
        asm!("cli", options(nomem, nostack));
    }
}

#[inline]
pub fn sti() {
    unsafe {
        asm!("sti", options(nomem, nostack));
    }
}

#[inline]
pub fn sti_nop() {
    unsafe {
        asm!("sti\nnop", options(nomem, nostack));
    }
}

#[inline]
pub fn sti_hlt() {
    unsafe {
        asm!("sti\nhlt", options(nomem, nostack));
    }
}

pub fn is_int_enabled() -> bool {
    get_flags() & RFLAGS_INT != 0
}

pub fn set_int_enabled(enabled: bool) {
    if enabled {
        sti_nop();
    } else {
        cli();
    }
}

#[derive(Debug)]
pub struct IntDisable {
    old_status: bool,
}

impl IntDisable {
    pub fn new() -> Self {
        let old_status = is_int_enabled();
        cli();
        IntDisable {
            old_status,
        }
    }

    pub fn old_is_enabled(&self) -> bool {
        self.old_status
    }
}

impl Drop for IntDisable {
    fn drop(&mut self) {
        set_int_enabled(self.old_status);
    }
}

#[inline]
pub fn outb(port: u16, data: u8) {
    unsafe {
        asm!("out dx, al", in("dx") port, in("al") data);
    }
}

#[inline]
pub fn outw(port: u16, data: u16) {
    unsafe {
        asm!("out dx, al", in("dx") port, in("ax") data);
    }
}

#[inline]
pub fn outd(port: u16, data: u32) {
    unsafe {
        asm!("out dx, al", in("dx") port, in("eax") data);
    }
}

#[inline]
pub fn inb(port: u16) -> u8 {
    let out;
    unsafe {
        asm!("in al, dx", in("dx") port, out("al") out);
    }
    out
}

#[inline]
pub fn inw(port: u16) -> u16 {
    let out;
    unsafe {
        asm!("in ax, dx", in("dx") port, out("ax") out);
    }
    out
}

#[inline]
pub fn ind(port: u16) -> u32 {
    let out;
    unsafe {
        asm!("in eax, dx", in("dx") port, out("eax") out);
    }
    out
}

// waits using the processor's io bus
#[inline]
pub fn io_wait(time: Duration) {
    for _ in 0..time.as_micros() {
        inb(0x80);
    }
}

/// Not write through, disables caching write through and write back when set
pub const CR0_NW: usize = 1 << 29;
/// Disables the cache
pub const CR0_CD: usize = 1 << 30;

#[inline]
pub fn get_cr0() -> usize {
    let out;
    unsafe {
        asm!("mov {}, cr0", out(reg) out, options(nomem, nostack));
    }
    out
}

#[inline]
pub fn set_cr0(n: usize) {
    unsafe {
        asm!("mov cr0, {}", in(reg) n, options(nomem, nostack));
    }
}

#[inline]
pub fn get_cr2() -> usize {
    let out;
    unsafe {
        asm!("mov {}, cr2", out(reg) out, options(nomem, nostack));
    }
    out
}

#[inline]
pub fn set_cr2(n: usize) {
    unsafe {
        asm!("mov cr2, {}", in(reg) n, options(nomem, nostack));
    }
}

#[inline]
pub fn get_cr3() -> usize {
    let out;
    unsafe {
        asm!("mov {}, cr3", out(reg) out, options(nomem, nostack));
    }
    out
}

#[inline]
pub fn set_cr3(n: usize) {
    unsafe {
        asm!("mov cr3, {}", in(reg) n, options(nomem, nostack));
    }
}

pub const CR4_PGE: usize = 1 << 7;
/// When set, disables certain privalidged instructions in usermode that
/// are usually only needed in virtual 8086 mode
pub const CR4_UMIP: usize = 1 << 11;
/// Disables kernel code from executing code in userspace pages
pub const CR4_SMEP: usize = 1 << 20;
/// Disables kernel code from reading or writing to userspace pages
pub const CR4_SMAP: usize = 1 << 21;

#[inline]
pub fn get_cr4() -> usize {
    let out;
    unsafe {
        asm!("mov {}, cr4", out(reg) out, options(nomem, nostack));
    }
    out
}

#[inline]
pub fn set_cr4(n: usize) {
    unsafe {
        asm!("mov cr4, {}", in(reg) n, options(nomem, nostack));
    }
}

#[inline]
pub fn get_rsp() -> usize {
    let out;
    unsafe {
        asm!("mov {}, rsp", out(reg) out, options(nomem));
    }
    out
}

#[inline]
pub fn invlpg(addr: usize) {
    unsafe {
        asm!("invlpg [{}]", in (reg) addr);
    }
}

extern "C" {
    fn asm_gs_addr() -> usize;
    pub fn asm_switch_thread(new_rsp: usize, new_addr_space: usize);
    pub fn asm_thread_init();

    pub fn asm_user_copy(dst: *mut u8, src: *const u8, count: usize) -> bool;
    pub fn asm_user_copy_fail() -> bool;
}

pub fn gs_addr() -> usize {
    unsafe { asm_gs_addr() }
}

/// Sets various miscalaneous cpu settings in control registers
pub fn config_cpu_settings() {
    // FIXME: set UMIP, SMEP, and SMAP
    // find out why setting these completely breaks things for some reason

    // enable global bit in page tables
    set_cr4(get_cr4() | CR4_PGE);

    // allow no execute bit to be set on page tables
    wrmsr(EFER_MSR, rdmsr(EFER_MSR) | EFER_EXEC_DISABLE);

    // clear cache disable and non write through bits
    // this enables the maximum level of caching
    set_cr0(get_cr0() & !(CR0_CD | CR0_NW));

    init_pat();
}