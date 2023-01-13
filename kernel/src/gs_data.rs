use core::sync::atomic::{AtomicUsize, Ordering};

use crate::alloc::root_alloc_ref;
use crate::arch::x64::{gs_addr, wrmsr, GSBASEK_MSR, GSBASE_MSR};
use crate::container::Box;
use crate::gdt::{Gdt, Tss};
use crate::int::idt::Idt;
use crate::sync::IMutex;

crate::make_id_type!(Prid);

/// This is cpu local data stored pointed to by the GS_BASE msr
/// Used for things like finding the kernel stack from a syscall and cpu local scheduler data
#[repr(C)]
#[derive(Debug)]
pub struct GsData {
    /// This contains the address of this gsdata struct itself
    /// 
    /// We need this because lea doesn't work with the gs register,
    /// so the assembly looks at this field and returns the pointer to the rust code
    pub self_addr: AtomicUsize,
    /// Used by assembly code to temporarily set the syscall return rip
    pub temp_syscall_return_rip: AtomicUsize,
    /// Id of the current processor
    pub prid: Prid,
    /// Interrupt descriptor table for current cpu
    pub idt: Idt,
    /// Global descriptor table for current cpu
    pub gdt: IMutex<Gdt>,
    /// Task state segment for current cpu
    pub tss: IMutex<Tss>,
}

impl GsData {
    pub fn set_self_addr(&self) {
        self.self_addr.store((self as *const _) as _, Ordering::Release);
    }
}

/// Sets the current cpu's local data
pub fn init(gs_data: GsData) {
    let gs_data = Box::new(gs_data, root_alloc_ref()).expect("Failed to allocate gs data struct");
    gs_data.set_self_addr();

    let (ptr, _) = Box::into_raw(gs_data);
    
    wrmsr(GSBASE_MSR, ptr as u64);
    wrmsr(GSBASEK_MSR, ptr as u64);
}

/// Gets the current cpu's local data
pub fn cpu_local_data() -> &'static GsData {
    unsafe { (gs_addr() as *const GsData).as_ref().unwrap() }
}

/// Gets the current processors id
pub fn prid() -> Prid {
    cpu_local_data().prid
}
