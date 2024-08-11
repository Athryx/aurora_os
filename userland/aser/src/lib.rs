//! Implements the aurora serialization format for message passing between processess
#![no_std]

#![feature(slice_take)]

#[cfg(feature = "alloc")]
extern crate alloc;

use core::fmt::Display;
use core::mem::size_of;
#[cfg(feature = "alloc")]
use alloc::string::{String, ToString};

use serde::{Serialize, Deserialize};
use sys::{CspaceTarget, CapId, cap_clone_inner, CapFlags, SysErr, CapabilityWeakness};
use thiserror_no_std::Error;
use num_enum::{TryFromPrimitive, IntoPrimitive};

mod byte_buf;
pub use byte_buf::ByteBuf;
mod capability_counter;
pub use capability_counter::count_capabilties;
mod capability_serializer;
mod capability_deserializer;
mod ser;
pub use ser::{Serializer, to_bytes, to_bytes_count_cap};
mod de;
pub use de::{Deserializer, from_bytes};
#[cfg(feature = "alloc")]
mod value;
#[cfg(feature = "alloc")]
pub use value::{Value, Integer, Float};

pub type Result<T> = core::result::Result<T, AserError>;

#[derive(Debug, Clone, Error, Serialize, Deserialize)]
pub enum AserError {
    #[cfg(feature = "alloc")]
    #[error("Serialize failed: {0}")]
    SerializeMessage(String),
    #[cfg(feature = "alloc")]
    #[error("Deserialize failed: {0}")]
    DeserializeMessage(String),

    // TODO: find a better way to make this cleaner
    // this is kind of bad to modify enum variants with feature and have a subtractive feature,
    // because features are unioned across build graph, but since the only time we don't use alloc
    // feature is in kernel, which won't match on these variants, it is ok
    #[cfg(not(feature = "alloc"))]
    #[error("Serialize implementation failed")]
    SerializeMessage,
    #[cfg(not(feature = "alloc"))]
    #[error("Deserialize implementation failed")]
    DeserializeMessage,

    #[error("Tried to serialize more capabilties than the serializer was set up for")]
    TooManyCapabilities,
    #[error("Expected a capability id")]
    ExpectedCapablity,
    #[error("Found multiple capabilties in one capability newtype")]
    MultipleCapabilties,
    #[error("Formatting display object as string failed")]
    FormattingError,

    #[error("Undexpected end of input")]
    EndOfInput,
    #[error("Invalid data type byte found")]
    InvalidDataType,
    #[error("Invalid utf-8 bytes encountered in string or character")]
    InvalidUtf8,
    #[error("Found a terminator byte where it was not expected")]
    UnexpectedTerminator,
    #[error("The specified enum variant should not have had any data")]
    EnumUnexpectedData,
    #[error("The specified capability index is out of range")]
    InvalidCapabilityIndex,
    #[error("The specified capability id is invalid")]
    InvalidCapabilityId,
    #[error("There are trailing characters on the end of the input")]
    TrailingInput,
}

#[cfg(feature = "alloc")]
impl serde::ser::Error for AserError {
    fn custom<T: Display>(msg: T) -> Self {
        Self::SerializeMessage(msg.to_string())
    }
}

#[cfg(feature = "alloc")]
impl serde::de::Error for AserError {
    fn custom<T: Display>(msg: T) -> Self {
        Self::DeserializeMessage(msg.to_string())
    }
}

#[cfg(not(feature = "alloc"))]
impl serde::ser::Error for AserError {
    fn custom<T: Display>(_msg: T) -> Self {
        Self::SerializeMessage
    }
}

#[cfg(not(feature = "alloc"))]
impl serde::de::Error for AserError {
    fn custom<T: Display>(_msg: T) -> Self {
        Self::DeserializeMessage
    }
}

/// Every serialized field has a byte to represent the type fof the field, this enum has all the types
#[derive(Debug, Clone, Copy, IntoPrimitive, TryFromPrimitive, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
enum DataType {
    Null = 0,
    True = 1,
    False = 2,
    I8 = 3,
    I16 = 4,
    I32 = 5,
    I64 = 6,
    I128 = 7,
    U8 = 8,
    U16 = 9,
    U32 = 10,
    U64 = 11,
    U128 = 12,
    F32 = 13,
    F64 = 14,
    Char = 15,
    /// All string variants are followed by n bit length, and string data
    /// 
    /// Length is length of string in **bytes**, not characters
    String8 = 16,
    String16 = 17,
    String32 = 18,
    String64 = 19,
    Bytes8 = 20,
    Bytes16 = 21,
    Bytes32 = 22,
    Bytes64 = 23,
    /// A newtype, used only for newtype structs, not newtype variants
    Newtype = 24,
    /// Represents an option with a value
    /// 
    /// This is required for option to deserialize right
    Some = 25,
    SequenceStart = 26,
    SequenceEnd = 27,
    MapStart = 28,
    MapEnd = 29,
    /// Enum member, followed by 32 bit index
    Variant = 30,
    /// Newtype enum member with a value, which can be any type
    /// 
    /// Followed by 32 bit inedex and another value
    VariantValue = 31,
    /// Followed by 16 bit index into capability array
    Capability = 32,
    Filler = 0xff,
}

#[derive(Debug, Error)]
pub enum AserCloneCapsError {
    #[error("Tried to clone an invalid capability")]
    InvalidCapabilityId,
    #[error("Undexpected end of input")]
    EndOfInput,

    #[error("An error was returned by as sytem call: {0}")]
    SysErr(#[from] SysErr),
}

type CloneCapsResult<T> = core::result::Result<T, AserCloneCapsError>;

fn get_usize(data: &[u8], index: usize) -> CloneCapsResult<usize> {
    let offset = index * size_of::<usize>();

    let bytes = data.get(offset..(offset + 8))
        .ok_or(AserCloneCapsError::EndOfInput)?;

    Ok(usize::from_le_bytes(bytes.try_into().unwrap()))
}

fn set_usize(data: &mut [u8], index: usize, num: usize) -> CloneCapsResult<()> {
    let offset = index * size_of::<usize>();

    let bytes = num.to_le_bytes();

    let data = data.get_mut(offset..(offset + 8))
        .ok_or(AserCloneCapsError::EndOfInput)?;

    data.copy_from_slice(&bytes);

    Ok(())
}

/// Clones all the capabilities in the serialized aser data to the given capability space
/// 
/// Updates the capability ids in the array to be the new ids
// FIXME: make this remove capabilities transfered on failure
pub fn clone_caps_to_cspace(cspace: CspaceTarget, data: &mut [u8]) -> CloneCapsResult<()> {
    let cap_count = get_usize(data, 0)?;

    for i in 1..(cap_count + 1) {
        let cap = get_usize(data, i)?;

        let cap_id = CapId::try_from(cap)
            .ok_or(AserCloneCapsError::InvalidCapabilityId)?;

        let new_cap_id = cap_clone_inner(
            cspace,
            CspaceTarget::Current,
            cap_id,
            CapFlags::all(),
            CapabilityWeakness::Current,
            false,
        )?;

        // panic safety: this index was just accessed
        set_usize(data, i, new_cap_id.into()).unwrap();
    }

    Ok(())
}