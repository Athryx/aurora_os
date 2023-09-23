use serde::{Serialize, Deserialize};
use strum::FromRepr;

use crate::{
    CapId,
    CapType,
    KResult,
    ThreadGroup,
    AddressSpace,
    CapabilitySpace,
    ThreadNewFlags,
    ThreadSuspendFlags,
    ThreadDestroyFlags,
    CspaceTarget,
    syscall,
    sysret_0,
    sysret_1,
    sysret_2,
};
use crate::syscall_nums::*;
use super::{Capability, Allocator, cap_destroy, WEAK_AUTO_DESTROY, INVALID_CAPID_MESSAGE};

#[derive(Debug, Serialize, Deserialize)]
pub struct Thread(CapId);

impl Capability for Thread {
    const TYPE: CapType = CapType::Thread;

    fn from_cap_id(cap_id: CapId) -> Option<Self> {
        if cap_id.cap_type() == CapType::Thread {
            Some(Thread(cap_id))
        } else {
            None
        }
    }

    fn cap_id(&self) -> CapId {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThreadStartMode {
    Ready,
    Suspended,
}

impl Thread {
    pub fn new(
        allocator: &Allocator,
        thread_group: &ThreadGroup,
        address_space: &AddressSpace,
        capability_space: &CapabilitySpace,
        rip: usize,
        rsp: usize,
        start_mode: ThreadStartMode,
    ) -> KResult<Self> {
        let flags = if start_mode == ThreadStartMode::Ready {
            ThreadNewFlags::THREAD_AUTOSTART
        } else {
            ThreadNewFlags::empty()
        };

        let cap_id = unsafe {
            sysret_1!(syscall!(
                THREAD_NEW,
                flags.bits() | WEAK_AUTO_DESTROY,
                allocator.as_usize(),
                thread_group.as_usize(),
                address_space.as_usize(),
                capability_space.as_usize(),
                rip,
                rsp
            ))?
        };

        Ok(Thread(CapId::try_from(cap_id).expect(INVALID_CAPID_MESSAGE)))
    }

    pub fn new_with_cspace(
        allocator: &Allocator,
        thread_group: &ThreadGroup,
        address_space: &AddressSpace,
        rip: usize,
        rsp: usize,
        start_mode: ThreadStartMode,
    ) -> KResult<(Self, CapabilitySpace)> {
        let flags = if start_mode == ThreadStartMode::Ready {
            ThreadNewFlags::THREAD_AUTOSTART
        } else {
            ThreadNewFlags::empty()
        } | ThreadNewFlags::CREATE_CAPABILITY_SPACE;

        let (thread, cspace) = unsafe {
            sysret_2!(syscall!(
                THREAD_NEW,
                flags.bits() | WEAK_AUTO_DESTROY,
                allocator.as_usize(),
                thread_group.as_usize(),
                address_space.as_usize(),
                0usize,
                rip,
                rsp
            ))?
        };

        Ok((
            Thread(CapId::try_from(thread).expect(INVALID_CAPID_MESSAGE)),
            CapabilitySpace::from_cap_id(CapId::try_from(cspace).expect(INVALID_CAPID_MESSAGE))
                .unwrap(),
        ))
    }

    pub fn yield_current() {
        unsafe {
            syscall!(
                THREAD_YIELD,
                0
            )
        }
    }

    pub fn destroy_current(&self) {
        unsafe {
            syscall!(
                THREAD_DESTROY,
                0
            )
        }
    }

    pub fn destroy(&self) -> KResult<()> {
        unsafe {
            sysret_0!(syscall!(
                THREAD_DESTROY,
                ThreadDestroyFlags::DESTROY_OTHER.bits() | WEAK_AUTO_DESTROY,
                self.as_usize()
            ))
        }
    }

    pub fn suspend() {
        unsafe {
            syscall!(
                THREAD_SUSPEND,
                0
            );
        }
    }
    
    pub fn suspend_until(nsec: u64) {
        unsafe {
            syscall!(
                THREAD_SUSPEND,
                ThreadSuspendFlags::SUSPEND_TIMEOUT.bits(),
                nsec
            );
        }
    }

    pub fn resume(&self) -> KResult<()> {
        unsafe {
            sysret_0!(syscall!(
                THREAD_RESUME,
                WEAK_AUTO_DESTROY,
                self.as_usize()
            ))
        }
    }
}

#[repr(usize)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, FromRepr)]
pub enum ThreadProperty {
    ThreadLocalPointer,
}

impl Thread {
    pub fn set_property(property: ThreadProperty, data: usize) -> KResult<()> {
        unsafe {
            sysret_0!(syscall!(
                THREAD_SET_PROPERTY,
                0,
                property as usize,
                data
            ))
        }
    }

    pub fn set_local_pointer(local_pointer: usize) {
        Self::set_property(ThreadProperty::ThreadLocalPointer, local_pointer)
            .expect("set local pointer should not fail");
    }
}

impl Drop for Thread {
    fn drop(&mut self) {
        let _ = cap_destroy(CspaceTarget::Current, self.0);
    }
}