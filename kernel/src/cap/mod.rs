use crate::alloc::OrigRef;
use crate::container::{Arc, Weak};
use crate::prelude::*;

mod capability_map;
pub use capability_map::*;
pub mod key;
pub mod memory;

pub use sys::{CapId, CapFlags, CapType};

pub trait CapObject {
    const TYPE: CapType;
}

#[derive(Debug)]
pub struct StrongCapability<T: CapObject> {
    object: Arc<T>,
    pub flags: CapFlags,
}

impl<T: CapObject> StrongCapability<T> {
    pub fn new(object: T, flags: CapFlags, allocer: OrigRef) -> KResult<Self> {
        Ok(StrongCapability {
            object: Arc::new(object, allocer)?,
            flags,
        })
    }

    pub fn inner(&self) -> &Arc<T> {
        &self.object
    }

    pub fn and_from_flags(cap: &Self, flags: CapFlags) -> Self {
        let mut out = cap.clone();
        out.flags &= flags;
        out
    }

    pub fn downgrade(&self) -> WeakCapability<T> {
        WeakCapability {
            object: Arc::downgrade(&self.object),
            flags: self.flags,
        }
    }

    pub fn object(&self) -> &T {
        &self.object
    }
}

// need explicit clone impl because derive only impls if T is clone
impl<T: CapObject> Clone for StrongCapability<T> {
    fn clone(&self) -> Self {
        StrongCapability {
            object: self.object.clone(),
            flags: self.flags,
        }
    }
}

#[derive(Debug)]
pub struct WeakCapability<T: CapObject> {
    object: Weak<T>,
    pub flags: CapFlags,
}

impl<T: CapObject> WeakCapability<T> {
    // fails if memory has been dropped or cap refcount is 0
    // NOTE: if do_refcount is false, this will succeeed if there is any arc pointing to the CapObject, even if there are no string capabilities
    pub fn upgrade(&self) -> Option<StrongCapability<T>> {
        Some(StrongCapability {
            object: self.object.upgrade()?,
            flags: self.flags,
        })
    }

    pub fn inner(&self) -> &Weak<T> {
        &self.object
    }

    pub fn and_from_flags(cap: &Self, flags: CapFlags) -> Self {
        let mut out: WeakCapability<T> = cap.clone();
        out.flags &= flags;
        out
    }
}

// default implementations of clone and drop are fine for this
impl<T: CapObject> Clone for WeakCapability<T> {
    fn clone(&self) -> Self {
        WeakCapability {
            object: self.object.clone(),
            flags: self.flags,
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
    pub fn flags(&self) -> CapFlags {
        match self {
            Self::Strong(cap) => cap.flags,
            Self::Weak(cap) => cap.flags,
        }
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
pub struct StaticCapability<T: CapObject + 'static> {
    object: &'static T,
    pub flags: CapFlags,
}

impl<T: CapObject + 'static> StaticCapability<T> {
    pub fn new(object: &'static T, flags: CapFlags) -> Self {
        Self {
            object,
            flags,
        }
    }

    pub fn and_from_flags(cap: &Self, flags: CapFlags) -> Self {
        let mut out = *cap;
        out.flags &= flags;
        out
    }

    pub fn object(&self) -> &'static T {
        self.object
    }
}

impl<T: CapObject + 'static> Clone for StaticCapability<T> {
    fn clone(&self) -> Self {
        StaticCapability {
            object: self.object,
            flags: self.flags,
        }
    }
}

// Do this here because derive copy doesn't work for some reason
impl<T: CapObject + 'static> Copy for StaticCapability<T> {}
