use core::sync::atomic::{AtomicUsize, Ordering};

use crate::prelude::*;
use crate::cap::{CapObject, StrongCapability, CapFlags};
use crate::alloc::OrigRef;
use crate::make_id_type;

make_id_type!(Pid);

static NEXT_PID: AtomicUsize = AtomicUsize::new(0);

pub struct Process {
    pid: Pid,
}

impl Process {
    pub fn new(allocer: OrigRef) -> KResult<StrongCapability<Self>> {
        StrongCapability::new(Process {
            pid: Pid::from(NEXT_PID.fetch_add(1, Ordering::Relaxed)),
        }, CapFlags::READ | CapFlags::PROD | CapFlags::WRITE, allocer)
    }
}

impl CapObject for Process {
    fn cap_drop(&self) {
    }
}