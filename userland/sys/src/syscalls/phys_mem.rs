use serde::{Serialize, Deserialize};

use crate::{
    CapId,
    CapType,
    CspaceTarget,
};
use super::{Capability, cap_destroy};

#[derive(Debug, Serialize, Deserialize)]
pub struct PhysMem(CapId);

impl Capability for PhysMem {
    const TYPE: CapType = CapType::PhysMem;

    fn cloned_new_id(&self, cap_id: CapId) -> Option<Self> {
        Self::from_cap_id(cap_id)
    }

    fn cap_id(&self) -> CapId {
        self.0
    }
}

impl PhysMem {
    pub fn from_cap_id(cap_id: CapId) -> Option<Self> {
        if cap_id.cap_type() == CapType::PhysMem {
            Some(PhysMem(cap_id))
        } else {
            None
        }
    }
}

impl Drop for PhysMem {
    fn drop(&mut self) {
        let _ = cap_destroy(CspaceTarget::Current, self.0);
    }
}