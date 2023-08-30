use crate::container::{Arc, Weak};
use crate::prelude::*;

mod capability_map;
pub use capability_map::*;
pub mod channel;
pub mod drop_check;
pub mod key;
pub mod memory;

pub use sys::{CapId, CapFlags, CapType};

pub trait CapObject {
    const TYPE: CapType;
}

#[derive(Debug)]
pub struct StrongCapability<T: CapObject> {
    object: Arc<T>,
    pub id: CapId,
}

impl<T: CapObject> StrongCapability<T> {
    pub fn new(object: Arc<T>, id: CapId) -> Self {
        StrongCapability {
            object,
            id,
        }
    }

    pub fn new_flags(object: Arc<T>, flags: CapFlags) -> Self {
        Self::new(object, CapId::null_flags(flags, false))
    }

    pub fn inner(&self) -> &Arc<T> {
        &self.object
    }

    pub fn into_inner(self) -> Arc<T> {
        self.object
    }

    pub fn object(&self) -> &T {
        &self.object
    }

    pub fn flags(&self) -> CapFlags {
        self.id.flags()
    }

    /// Returns true if this capability references a strong capability in the capability map
    pub fn references_strong(&self) -> bool {
        !self.id.is_weak()
    }

    pub fn downgrade(&self) -> WeakCapability<T> {
        WeakCapability {
            object: Arc::downgrade(&self.object),
            id: self.id,
        }
    }
}

// need explicit clone impl because derive only impls if T is clone
impl<T: CapObject> Clone for StrongCapability<T> {
    fn clone(&self) -> Self {
        StrongCapability {
            object: self.object.clone(),
            id: self.id,
        }
    }
}

#[derive(Debug)]
pub struct WeakCapability<T: CapObject> {
    object: Weak<T>,
    pub id: CapId,
}

impl<T: CapObject> WeakCapability<T> {
    pub fn inner(&self) -> &Weak<T> {
        &self.object
    }

    pub fn into_inner(self) -> Weak<T> {
        self.object
    }

    pub fn flags(&self) -> CapFlags {
        self.id.flags()
    }

    pub fn upgrade(&self) -> Option<StrongCapability<T>> {
        Some(StrongCapability {
            object: self.object.upgrade()?,
            id: self.id,
        })
    }
}

// default implementations of clone and drop are fine for this
impl<T: CapObject> Clone for WeakCapability<T> {
    fn clone(&self) -> Self {
        WeakCapability {
            object: self.object.clone(),
            id: self.id,
        }
    }
}

/// A capability that is either strong or weak
#[derive(Debug)]
pub enum Capability<T: CapObject> {
    Strong(StrongCapability<T>),
    Weak(WeakCapability<T>),
}

impl<T: CapObject> Capability<T> {
    pub fn id(&self) -> CapId {
        match self {
            Self::Strong(cap) => cap.id,
            Self::Weak(cap) => cap.id,
        }
    }

    pub fn set_id(&mut self, id: CapId) {
        match self {
            Self::Strong(cap) => cap.id = id,
            Self::Weak(cap) => cap.id = id,
        }
    }

    pub fn flags(&self) -> CapFlags {
        self.id().flags()
    }

    pub fn is_weak(&self) -> bool {
        matches!(self, Self::Weak(_))
    }
}

impl<T: CapObject> Clone for Capability<T> {
    fn clone(&self) -> Self {
        match self {
            Self::Strong(cap) => Self::Strong(cap.clone()),
            Self::Weak(cap) => Self::Weak(cap.clone()),
        }
    }
}

/// A capability that points to certain objects that are static and always exist in the kernel
/// From the userspace perspective, these capabilites act like normal capabilties, except the object is not dropped ever
#[derive(Clone, Copy)]
pub struct StaticCapability<T: CapObject + 'static> {
    object: &'static T,
    pub id: CapId,
}

impl<T: CapObject + 'static> StaticCapability<T> {
    pub fn new(object: &'static T, id: CapId) -> Self {
        Self {
            object,
            id,
        }
    }

    pub fn object(&self) -> &'static T {
        self.object
    }

    pub fn flags(&self) -> CapFlags {
        self.id.flags()
    }
}