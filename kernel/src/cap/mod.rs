use bitflags::bitflags;

use crate::alloc::OrigRef;
use crate::container::{Arc, Weak};
use crate::make_id_type_no_from;
use crate::prelude::*;

mod capability_map;
pub use capability_map::*;

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
    Process = 1,
    Memory = 2,
    Lock = 3,
    EventPool = 4,
    Channel = 5,
    Key = 6,
    Interrupt = 7,
    Port = 8,
    Spawner = 9,
    Allocator = 10,
    RootOom = 11,
    MmioAllocator = 12,
    IntAllocator = 13,
    PortAllocator = 14,
}

impl CapType {
    pub fn from(n: usize) -> Option<Self> {
        Some(match n {
            1 => Self::Process,
            2 => Self::Memory,
            3 => Self::Lock,
            4 => Self::EventPool,
            5 => Self::Channel,
            6 => Self::Key,
            7 => Self::Interrupt,
            8 => Self::Port,
            9 => Self::Spawner,
            10 => Self::Allocator,
            11 => Self::RootOom,
            12 => Self::MmioAllocator,
            13 => Self::IntAllocator,
            14 => Self::PortAllocator,
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
        let bits = get_bits(n, 5..9);
        if bits == 0 || bits > 14 {
            None
        } else {
            Some(CapId(n))
        }
    }

    /// Creates a valid CapId from the given `cap_type`, `flags`, `is_weak`, and `base_id`
    /// 
    /// `base_id` should be a unique integer in order for this id to be unique
    pub fn new(cap_type: CapType, flags: CapFlags, is_weak: bool, base_id: usize) -> Self {
        CapId(flags.bits | ((is_weak as usize) << 4) | (cap_type.as_usize() << 5) | (base_id << 9))
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
