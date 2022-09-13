use core::sync::atomic::AtomicUsize;

use crate::container::Box;
use crate::alloc::root_alloc_ref;
use crate::arch::x64::{gs_addr, wrmsr, GSBASE_MSR, GSBASEK_MSR};

crate::make_id_type!(Prid);

/// This is cpu local data stored pointed to by the GS_BASE msr
/// Used for things like finding the kernel stack from a syscall and cpu local scheduler data
#[repr(C)]
#[derive(Debug)]
pub struct GsData {
    /// Used by assembly code to temporarily set the syscall return rip
    pub temp_syscall_return_rip: AtomicUsize,
    /// Id of the current processor
    pub prid: Prid,
}

pub fn init(gs_data: GsData) {
    let (ptr, _) = Box::into_raw(
        Box::new(gs_data, root_alloc_ref())
            .expect("Failed to allocate gs data struct")
    );

    wrmsr(GSBASE_MSR, ptr as u64);
    wrmsr(GSBASEK_MSR, ptr as u64);
}

pub fn cpu_local_data() -> &'static GsData {
    unsafe {
        (gs_addr() as *const GsData).as_ref().unwrap()
    }
}

pub fn prid() -> Prid {
    cpu_local_data().prid
}