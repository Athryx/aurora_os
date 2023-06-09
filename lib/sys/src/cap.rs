use bitflags::bitflags;
use bit_utils::get_bits;
use serde::{Serialize, Deserialize, de::{Visitor, Error}};
use aser::CAPABILTY_NEWTYPE_NAME;

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
    RecvPool = 6,
    Key = 7,
    Interrupt = 8,
    Port = 9,
    Spawner = 10,
    Allocator = 11,
    RootOom = 12,
    MmioAllocator = 13,
    IntAllocator = 14,
    PortAllocator = 15,
}

impl CapType {
    pub fn from(n: usize) -> Option<Self> {
        Some(match n {
            1 => Self::Process,
            2 => Self::Memory,
            3 => Self::Lock,
            4 => Self::EventPool,
            5 => Self::Channel,
            6 => Self::RecvPool,
            7 => Self::Key,
            8 => Self::Interrupt,
            9 => Self::Port,
            10 => Self::Spawner,
            11 => Self::Allocator,
            12 => Self::RootOom,
            13 => Self::MmioAllocator,
            14 => Self::IntAllocator,
            15 => Self::PortAllocator,
            _ => return None,
        })
    }

    pub fn as_usize(&self) -> usize {
        *self as usize
    }
}

#[repr(transparent)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CapId(usize);

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

    /// Creates a null capid with the given flags
    /// 
    /// Used when a capid has not yet been asigned to an object, but it has some specified flags
    pub fn null_flags(flags: CapFlags, is_weak: bool) -> Self {
        CapId(flags.bits | ((is_weak as usize) << 4))
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

impl From<CapId> for usize {
    fn from(cap_id: CapId) -> Self {
        cap_id.0
    }
}

impl Serialize for CapId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer {
        serializer.serialize_newtype_struct("__aser_cap", &self.0)
    }
}

impl<'de> Deserialize<'de> for CapId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de> {
        deserializer.deserialize_newtype_struct(CAPABILTY_NEWTYPE_NAME, CapIdVisitor)
    }
}

struct CapIdVisitor;

impl<'de> Visitor<'de> for CapIdVisitor {
    type Value = CapId;

    fn expecting(&self, formatter: &mut core::fmt::Formatter) -> core::fmt::Result {
        formatter.write_str("a valid 64 bit capability id")
    }

    fn visit_newtype_struct<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>, {
        let id = u64::deserialize(deserializer)? as usize;

        CapId::try_from(id).ok_or(D::Error::custom("invalid capid"))
    }
}