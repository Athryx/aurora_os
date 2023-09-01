use serde::{Serialize, Deserialize};

use crate::{
    CapId,
    CapType,
    CapFlags,
    KResult,
    syscall,
    sysret_0,
    sysret_1,
};
use crate::syscall_nums::*;
use super::{Capability, Allocator, Key, WEAK_AUTO_DESTROY, INVALID_CAPID_MESSAGE};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Spawner(CapId);

impl Capability for Spawner {
    const TYPE: CapType = CapType::Spawner;

    fn from_cap_id(cap_id: CapId) -> Option<Self> {
        if cap_id.cap_type() == CapType::Spawner {
            Some(Spawner(cap_id))
        } else {
            None
        }
    }

    fn cap_id(&self) -> CapId {
        self.0
    }
}

impl Spawner {
    pub fn new(flags: CapFlags, allocator: Allocator, spawn_key: Key) -> KResult<Self> {
        unsafe {
            sysret_1!(syscall!(
                SPAWNER_NEW,
                flags.bits() as u32 | WEAK_AUTO_DESTROY,
                allocator.as_usize(),
                spawn_key.as_usize()
            )).map(|num| Spawner(CapId::try_from(num).expect(INVALID_CAPID_MESSAGE)))
        }
    }

    pub fn kill_all(&self) -> KResult<()> {
        unsafe {
            sysret_0!(syscall!(
                SPAWNER_KILL_ALL,
                WEAK_AUTO_DESTROY,
                self.as_usize()
            ))
        }
    }
}