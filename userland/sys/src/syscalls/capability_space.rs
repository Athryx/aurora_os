use serde::{Serialize, Deserialize};

use crate::{
    CapId,
    CapType,
    CspaceTarget,
};
use super::{Capability, cap_destroy};

#[derive(Debug, Serialize, Deserialize)]
pub struct CapabilitySpace(CapId);

impl Capability for CapabilitySpace {
    const TYPE: CapType = CapType::CapabilitySpace;

    fn cloned_new_id(&self, cap_id: CapId) -> Option<Self> {
        Self::from_cap_id(cap_id)
    }

    fn cap_id(&self) -> CapId {
        self.0
    }
}

impl CapabilitySpace {
    pub fn from_cap_id(cap_id: CapId) -> Option<Self> {
        if cap_id.cap_type() == CapType::CapabilitySpace {
            Some(CapabilitySpace(cap_id))
        } else {
            None
        }
    }
}

impl Drop for CapabilitySpace {
    fn drop(&mut self) {
        let _ = cap_destroy(CspaceTarget::Current, self.0);
    }
}