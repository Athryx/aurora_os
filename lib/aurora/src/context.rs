use serde::{Serialize, Deserialize};
use sys::{ThreadGroup, AddressSpace, CapabilitySpace, Allocator, Capability, KResult, cap_clone, CspaceTarget, CapFlags};

use crate::this_context;

#[derive(Debug)]
pub struct Context {
    pub thread_group: ThreadGroup,
    pub address_space: AddressSpace,
    pub capability_space: CapabilitySpace,
    pub allocator: Allocator,
}

impl Context {
    /*pub fn cspace_target(&self) -> CspaceTarget {
        if self.is_current_process() {
            CspaceTarget::Current
        } else {
            CspaceTarget::Other(self.process)
        }
    }

    pub fn is_current_process(&self) -> bool {
        this_context().process == self.process
    }

    /// If this context is not the current process, clones a capability from the current process to the new process
    /// 
    /// Returns Ok(Some(T)) if the capability was copied to the new process, Ok(None) if it was not, or Err(_) if an error occured
    pub fn clone_capability_to<T: Capability + Copy>(&self, capability: T) -> KResult<Option<T>> {
        if !self.is_current_process() {
            Ok(Some(cap_clone(
                ProcessTarget::Other(self.process),
                ProcessTarget::Current,
                capability,
                CapFlags::all(),
            )?))
        } else {
            Ok(None)
        }
    }*/
}