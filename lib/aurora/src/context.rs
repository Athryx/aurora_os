use serde::{Serialize, Deserialize};
use sys::{Process, Allocator, Spawner, Capability, KResult, cap_clone, ProcessTarget, CapFlags};

use crate::this_context;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Context {
    pub process: Process,
    pub allocator: Allocator,
    pub spawner: Spawner,
}

impl Context {
    pub fn process_target(&self) -> ProcessTarget {
        if self.is_current_process() {
            ProcessTarget::Current
        } else {
            ProcessTarget::Other(self.process)
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
    }
}