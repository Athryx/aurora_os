use serde::{Serialize, Deserialize};

use crate::{
    CapId,
    CapType,
    CapFlags,
    KResult,
    syscall,
    sysret_1,
};
use crate::syscall_nums::*;
use super::{Capability, Allocator, WEAK_AUTO_DESTROY, INVALID_CAPID_MESSAGE};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Key(CapId);

impl Capability for Key {
    const TYPE: CapType = CapType::Key;

    fn from_cap_id(cap_id: CapId) -> Option<Self> {
        if cap_id.cap_type() == CapType::Key {
            Some(Key(cap_id))
        } else {
            None
        }
    }

    fn cap_id(&self) -> CapId {
        self.0
    }
}

impl Key {
    pub fn new(flags: CapFlags, allocator: Allocator) -> KResult<Self> {
        unsafe {
            sysret_1!(syscall!(
                KEY_NEW,
                flags.bits() as u32 | WEAK_AUTO_DESTROY,
                allocator.as_usize()
            )).map(|num| Key(CapId::try_from(num).expect(INVALID_CAPID_MESSAGE)))
        }
    }

    pub fn key_id(&self) -> KResult<usize> {
        unsafe {
            sysret_1!(syscall!(
                KEY_ID,
                WEAK_AUTO_DESTROY,
                self.as_usize()
            ))
        }
    }
}