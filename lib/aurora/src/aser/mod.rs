//! Implements the aurora serialization format for message passing between processess

use core::fmt::Display;

use thiserror_no_std::Error;
use num_enum::{TryFromPrimitive, IntoPrimitive};

use crate::prelude::*;

mod ser;
pub use ser::{Serializer, to_bytes};
mod de;
pub use de::{Deserializer, from_bytes};

pub type Result<T> = core::result::Result<T, AserError>;

#[derive(Debug, Error)]
pub enum AserError {
    #[error("Serialize failed: {0}")]
    SerializeMessage(String),
    #[error("Deserialize failed: {0}")]
    DeserializeMessage(String),

    #[error("Tried to serialize more capabilties than the serializer was set up for")]
    TooManyCapabilities,
    #[error("Expected a capability id")]
    ExpectedCapablity,

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
    #[error("There are trailing characters on the end of the input")]
    TrailingInput,
}

impl serde::ser::Error for AserError {
    fn custom<T: Display>(msg: T) -> Self {
        Self::SerializeMessage(msg.to_string())
    }
}

impl serde::de::Error for AserError {
    fn custom<T: Display>(msg: T) -> Self {
        Self::DeserializeMessage(msg.to_string())
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
    SequenceStart = 24,
    SequenceEnd = 25,
    MapStart = 26,
    MapEnd = 27,
    /// Enum member, followed by 32 bit index
    Variant = 28,
    /// Newtype enum member with a value, which can be any type
    /// 
    /// Followed by 32 bit inedex and another value
    VariantValue = 29,
    /// Followed by 16 bit index into capability array
    Capability = 30,
}