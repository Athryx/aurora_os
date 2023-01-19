use core::sync::atomic::{AtomicUsize, Ordering};

use crate::alloc::OrigRef;
use crate::cap::{CapFlags, CapObject, StrongCapability};
use crate::make_id_type;
use crate::prelude::*;

mod vmem_manager;

make_id_type!(Pid);

static NEXT_PID: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug)]
pub struct Process {
    pid: Pid,
}

impl Process {
    pub fn new(allocer: OrigRef) -> KResult<StrongCapability<Self>> {
        StrongCapability::new(
            Process {
                pid: Pid::from(NEXT_PID.fetch_add(1, Ordering::Relaxed)),
            },
            CapFlags::READ | CapFlags::PROD | CapFlags::WRITE,
            allocer,
        )
    }
}

impl CapObject for Process {
    fn cap_drop(&self) {}
}