use bitflags::bitflags;

use crate::alloc::OrigRef;
use crate::container::{Arc, Weak};
use crate::make_id_type_no_from;
use crate::prelude::*;

bitflags! {
    pub struct CapFlags: usize {
        const READ = 1;
        const PROD = 1 << 1;
        const WRITE = 1 << 2;
        const UPGRADE = 1 << 3;
    }
}

#[repr(usize)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapType {
    Process,
    Memory,
    Event,
    Channel,
    Key,
    Interrupt,
    Port,
    Spawner,
    Allocator,
    RootOom,
    MmioAllocator,
    IntAllocator,
    PortAllocator,
}

impl CapType {
    pub fn from(n: usize) -> Option<Self> {
        Some(match n {
            0 => Self::Process,
            1 => Self::Memory,
            2 => Self::Event,
            3 => Self::Channel,
            4 => Self::Key,
            5 => Self::Interrupt,
            6 => Self::Port,
            7 => Self::Spawner,
            8 => Self::Allocator,
            9 => Self::RootOom,
            10 => Self::MmioAllocator,
            11 => Self::IntAllocator,
            12 => Self::PortAllocator,
            _ => return None,
        })
    }

    pub fn as_usize(&self) -> usize {
        *self as usize
    }
}

make_id_type_no_from!(CapId);

impl CapId {
    pub fn try_from(n: usize) -> Option<Self> {
        // fail if invalid type of cap object
        if get_bits(n, 5..9) > 12 {
            None
        } else {
            Some(CapId(n))
        }
    }

    pub fn flags(&self) -> CapFlags {
        CapFlags::from_bits_truncate(self.0)
    }

    pub fn is_weak(&self) -> bool {
        get_bits(self.0, 4..5) == 1
    }

    pub fn cap_type(&self) -> CapType {
        // panic safety: CapId will always have valid metadata, this is checked in the constructor
        CapType::from(get_bits(self.0, 5..9)).unwrap()
    }
}

impl Default for CapId {
    fn default() -> Self {
        // panic safety: 0 is a valid CapType
        Self::try_from(0).unwrap()
    }
}

pub trait CapObject {}

#[derive(Debug)]
pub struct StrongCapability<T: CapObject> {
    object: Arc<T>,
    flags: CapFlags,
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

    pub fn flags(&self) -> CapFlags {
        self.flags
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
    flags: CapFlags,
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

    pub fn flags(&self) -> CapFlags {
        self.flags
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

/// A capability that points to certain objects that are static and always exist in the kernel
/// From the userspace perspective, these capabilites act like normal capabilties, except the object is not dropped ever
pub struct StaticCapability<T: CapObject + 'static> {
    object: &'static T,
    flags: CapFlags,
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

    pub fn flags(&self) -> CapFlags {
        self.flags
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
