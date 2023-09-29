use bitflags::bitflags;
use bit_utils::get_bits;
use serde::{Serialize, Deserialize, de::{Visitor, Error}};

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
    Thread = 1,
    ThreadGroup = 2,
    AddressSpace = 3,
    CapabilitySpace = 4,
    Memory = 5,
    Lock = 6,
    EventPool = 7,
    Channel = 8,
    MessageCapacity = 9,
    Key = 10,
    Interrupt = 11,
    Port = 12,
    Allocator = 13,
    DropCheck = 14,
    DropCheckReciever = 15,
    RootOom = 16,
    MmioAllocator = 17,
    IntAllocator = 18,
    PortAllocator = 19,
}

impl CapType {
    pub fn from(n: usize) -> Option<Self> {
        Some(match n {
            1 => Self::Thread,
            2 => Self::ThreadGroup,
            3 => Self::AddressSpace,
            4 => Self::CapabilitySpace,
            5 => Self::Memory,
            6 => Self::Lock,
            7 => Self::EventPool,
            8 => Self::Channel,
            9 => Self::MessageCapacity,
            10 => Self::Key,
            11 => Self::Interrupt,
            12 => Self::Port,
            13 => Self::Allocator,
            14 => Self::DropCheck,
            15 => Self::DropCheckReciever,
            16 => Self::RootOom,
            17 => Self::MmioAllocator,
            18 => Self::IntAllocator,
            19 => Self::PortAllocator,
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
        let bits = get_bits(n, 5..10);
        let _cap_type = CapType::from(bits)?;

        Some(CapId(n))
    }

    /// Creates a valid CapId from the given `cap_type`, `flags`, `is_weak`, and `base_id`
    /// 
    /// `base_id` should be a unique integer in order for this id to be unique
    pub fn new(cap_type: CapType, flags: CapFlags, is_weak: bool, base_id: usize) -> Self {
        CapId(flags.bits | ((is_weak as usize) << 4) | (cap_type.as_usize() << 5) | (base_id << 10))
    }

    /// Creates a null capid with the given flags
    /// 
    /// Used when a capid has not yet been asigned to an object, but it has some specified flags
    pub fn null_flags(flags: CapFlags, is_weak: bool) -> Self {
        CapId(flags.bits | ((is_weak as usize) << 4))
    }

    pub fn null() -> Self {
        CapId(0)
    }

    pub fn flags(&self) -> CapFlags {
        CapFlags::from_bits_truncate(self.0)
    }

    pub fn is_weak(&self) -> bool {
        get_bits(self.0, 4..5) == 1
    }

    pub fn cap_type(&self) -> CapType {
        // panic safety: CapId will always have valid metadata, this is checked in the constructor
        CapType::from(get_bits(self.0, 5..10)).unwrap()
    }


    /// Any newtype struct with this name will be treated as a capability by aser
    /// 
    /// This name is reserved for other structs
    pub const SERIALIZE_NEWTYPE_NAME: &str = "__aser_cap";
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
        serializer.serialize_newtype_struct(Self::SERIALIZE_NEWTYPE_NAME, &self.0)
    }
}

impl<'de> Deserialize<'de> for CapId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de> {
        deserializer.deserialize_newtype_struct(Self::SERIALIZE_NEWTYPE_NAME, CapIdVisitor)
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