use core::fmt::{self, Display};

use derive_more::Display;

use bitflags::bitflags;
use bit_utils::get_bits;
use serde::{Serialize, Deserialize, de::{Visitor, Error, EnumAccess, VariantAccess}};

bitflags! {
    #[derive(Debug, Clone, Copy)]
    pub struct CapFlags: usize {
        const READ = 1;
        const PROD = 1 << 1;
        const WRITE = 1 << 2;
        const UPGRADE = 1 << 3;
    }
}

#[repr(usize)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Display)]
pub enum CapType {
    Null = 0,
    Thread = 1,
    ThreadGroup = 2,
    AddressSpace = 3,
    CapabilitySpace = 4,
    Memory = 5,
    Lock = 6,
    EventPool = 7,
    Channel = 8,
    Reply = 9,
    MessageCapacity = 10,
    Key = 11,
    Allocator = 12,
    DropCheck = 13,
    DropCheckReciever = 14,
    RootOom = 15,
    MmioAllocator = 16,
    PhysMem = 17,
    IntAllocator = 18,
    Interrupt = 19,
}

impl CapType {
    pub fn from(n: usize) -> Option<Self> {
        Some(match n {
            0 => Self::Null,
            1 => Self::Thread,
            2 => Self::ThreadGroup,
            3 => Self::AddressSpace,
            4 => Self::CapabilitySpace,
            5 => Self::Memory,
            6 => Self::Lock,
            7 => Self::EventPool,
            8 => Self::Channel,
            9 => Self::Reply,
            10 => Self::MessageCapacity,
            11 => Self::Key,
            12 => Self::Allocator,
            13 => Self::DropCheck,
            14 => Self::DropCheckReciever,
            15 => Self::RootOom,
            16 => Self::MmioAllocator,
            17 => Self::PhysMem,
            18 => Self::IntAllocator,
            19 => Self::Interrupt,
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
        CapId(flags.bits() | ((is_weak as usize) << 4) | (cap_type.as_usize() << 5) | (base_id << 10))
    }

    /// Creates a null capid with the given flags
    /// 
    /// Used when a capid has not yet been asigned to an object, but it has some specified flags
    pub fn null_flags(flags: CapFlags, is_weak: bool) -> Self {
        CapId(flags.bits() | ((is_weak as usize) << 4))
    }

    pub fn null() -> Self {
        CapId(0)
    }

    pub fn is_null(&self) -> bool {
        self.0 == 0
    }

    pub fn flags(&self) -> CapFlags {
        CapFlags::from_bits_truncate(self.0)
    }

    pub fn is_weak(&self) -> bool {
        get_bits(self.0, 4..5) == 1
    }

    pub fn base_id(&self) -> usize {
        get_bits(self.0, 5..64)
    }

    /// # Panics
    /// 
    /// Panics if this capability is null
    // FIXME: introduce null to CapType enum
    pub fn cap_type(&self) -> CapType {
        // panic safety: CapId will always have valid metadata, this is checked in the constructor
        CapType::from(get_bits(self.0, 5..10)).unwrap_or(CapType::Null)
    }

    /// Newtype enum with this variant will be treated as a capability by aser
    /// 
    /// This variant is reserved for other enums
    pub const SERIALIZE_ENUM_VARIANT: u32 = 2987132124;
}

impl From<CapId> for usize {
    fn from(cap_id: CapId) -> Self {
        cap_id.0
    }
}

impl Display for CapId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_null() {
            write!(f, "<null cap id>")
        } else {
            let flags = self.flags();

            let r = if flags.contains(CapFlags::READ) { 'R' } else { '-' };
            let p = if flags.contains(CapFlags::PROD) { 'P' } else { '-' };
            let w = if flags.contains(CapFlags::WRITE) { 'W' } else { '-' };
            let u = if flags.contains(CapFlags::UPGRADE) { 'U' } else { '-' };

            write!(
                f,
                "{:x}({}:{}{}{}{})",
                self.0,
                self.cap_type(),
                r, p, w, u,
            )
        }
    }
}

impl Serialize for CapId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer {
        serializer.serialize_newtype_variant(
            "CapId",
            Self::SERIALIZE_ENUM_VARIANT,
            "CapId",
            &self.0,
        )
    }
}

impl<'de> Deserialize<'de> for CapId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de> {
        deserializer.deserialize_enum("CapId", &[], CapIdVisitor)
    }
}

struct CapIdVisitor;

impl<'de> Visitor<'de> for CapIdVisitor {
    type Value = CapId;

    fn expecting(&self, formatter: &mut core::fmt::Formatter) -> core::fmt::Result {
        crate::dprintln!("expecting");
        formatter.write_str("a valid 64 bit capability id")
    }

    fn visit_enum<A>(self, data: A) -> Result<Self::Value, A::Error>
    where
        A: EnumAccess<'de> {
        let (varient_index, varient_access) = data.variant::<u32>()?;
        if varient_index != CapId::SERIALIZE_ENUM_VARIANT {
            Err(A::Error::custom("invalid capid enum variant"))
        } else {
            let cap_id = varient_access.newtype_variant::<u64>()?;

            CapId::try_from(cap_id as usize).ok_or(A::Error::custom("invalid capid"))
        }
    }

    fn visit_newtype_struct<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>, {
        let id = u64::deserialize(deserializer)? as usize;

        CapId::try_from(id).ok_or(D::Error::custom("invalid capid"))
    }
}