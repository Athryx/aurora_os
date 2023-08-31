use core::cmp::{Ord, Ordering};
use alloc::collections::BTreeMap;
use alloc::{string::String, vec::Vec, boxed::Box};

use sys::CapId;
use serde::{
    Serialize,
    Serializer,
    Deserialize,
    Deserializer,
    ser::{SerializeTuple, SerializeMap},
    de::{Visitor, SeqAccess, MapAccess, EnumAccess, VariantAccess},
};

use super::AserError;

mod value_serializer;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Integer {
    I8(i8),
    I16(i16),
    I32(i32),
    I64(i64),
    I128(i128),
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    U128(u128),
}

impl Integer {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match *self {
            Self::I8(n) => serializer.serialize_i8(n),
            Self::I16(n) => serializer.serialize_i16(n),
            Self::I32(n) => serializer.serialize_i32(n),
            Self::I64(n) => serializer.serialize_i64(n),
            Self::I128(n) => serializer.serialize_i128(n),
            Self::U8(n) => serializer.serialize_u8(n),
            Self::U16(n) => serializer.serialize_u16(n),
            Self::U32(n) => serializer.serialize_u32(n),
            Self::U64(n) => serializer.serialize_u64(n),
            Self::U128(n) => serializer.serialize_u128(n),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Float {
    F32(f32),
    F64(f64),
}

impl Float {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match *self {
            Self::F32(n) => serializer.serialize_f32(n),
            Self::F64(n) => serializer.serialize_f64(n),
        }
    }
}

/// This is needed for value to work as a BtreeMap key
impl PartialEq for Float {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::F32(a), Self::F32(b)) => a.to_bits() == b.to_bits(),
            (Self::F64(a), Self::F64(b)) => a.to_bits() == b.to_bits(),
            _ => false,
        }
    }
}

impl Eq for Float {}

impl PartialOrd for Float {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Float {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (Self::F32(a), Self::F32(b)) => a.to_bits().cmp(&b.to_bits()),
            (Self::F64(a), Self::F64(b)) => a.to_bits().cmp(&b.to_bits()),
            (Self::F32(_), Self::F64(_)) => Ordering::Less,
            (Self::F64(_), Self::F32(_)) => Ordering::Greater,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Value {
    Null,
    Bool(bool),
    Integer(Integer),
    Float(Float),
    Char(char),
    String(String),
    Bytes(Vec<u8>),
    Sequence(Vec<Value>),
    Map(BTreeMap<Value, Value>),
    Capability(CapId),
    EnumVariant {
        variant_index: u32,
        value: Box<Value>,
    },
}

impl Value {
    pub fn from_serialize<T: Serialize>(data: T) -> Result<Self, AserError> {
        data.serialize(value_serializer::ValueSerializer)
    }
}

impl Serialize for Value {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Null => serializer.serialize_none(),
            Self::Bool(v) => serializer.serialize_bool(*v),
            Self::Integer(n) => n.serialize(serializer),
            Self::Float(n) => n.serialize(serializer),
            Self::Char(c) => serializer.serialize_char(*c),
            Self::String(s) => serializer.serialize_str(&s),
            Self::Bytes(data) => serializer.serialize_bytes(&data),
            Self::Sequence(data) => {
                let mut seq_serializer = serializer.serialize_tuple(data.len())?;

                for value in data {
                    seq_serializer.serialize_element(value)?;
                }

                seq_serializer.end()
            },
            Self::Map(map) => {
                let mut map_serializer = serializer.serialize_map(Some(map.len()))?;

                for (key, value) in map.iter() {
                    map_serializer.serialize_key(key)?;
                    map_serializer.serialize_value(value)?;
                }

                map_serializer.end()
            },
            Self::Capability(cap_id) => serializer.serialize_newtype_struct(
                CapId::SERIALIZE_NEWTYPE_NAME,
                &usize::from(*cap_id)
            ),
            Self::EnumVariant {
                variant_index,
                value,
            } => {
                if &**value == &Self::Null {
                    serializer.serialize_unit_variant("", *variant_index, "")
                } else {
                    serializer.serialize_newtype_variant("", *variant_index, "", &value)
                }
            }
        }
    }
}

impl<'de> Deserialize<'de> for Value {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(ValueVisitor)
    }
}

struct ValueVisitor;

impl<'de> Visitor<'de> for ValueVisitor {
    type Value = Value;

    fn expecting(&self, formatter: &mut core::fmt::Formatter) -> core::fmt::Result {
        formatter.write_str("any valid aser value")
    }

    fn visit_bool<E>(self, v: bool) -> Result<Self::Value, E> {
        Ok(Value::Bool(v))
    }

    fn visit_i8<E>(self, v: i8) -> Result<Self::Value, E> {
        Ok(Value::Integer(Integer::I8(v)))
    }

    fn visit_i16<E>(self, v: i16) -> Result<Self::Value, E> {
        Ok(Value::Integer(Integer::I16(v)))
    }

    fn visit_i32<E>(self, v: i32) -> Result<Self::Value, E> {
        Ok(Value::Integer(Integer::I32(v)))
    }

    fn visit_i64<E>(self, v: i64) -> Result<Self::Value, E> {
        Ok(Value::Integer(Integer::I64(v)))
    }

    fn visit_i128<E>(self, v: i128) -> Result<Self::Value, E> {
        Ok(Value::Integer(Integer::I128(v)))
    }

    fn visit_u8<E>(self, v: u8) -> Result<Self::Value, E> {
        Ok(Value::Integer(Integer::U8(v)))
    }

    fn visit_u16<E>(self, v: u16) -> Result<Self::Value, E> {
        Ok(Value::Integer(Integer::U16(v)))
    }

    fn visit_u32<E>(self, v: u32) -> Result<Self::Value, E> {
        Ok(Value::Integer(Integer::U32(v)))
    }

    fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E> {
        Ok(Value::Integer(Integer::U64(v)))
    }

    fn visit_u128<E>(self, v: u128) -> Result<Self::Value, E> {
        Ok(Value::Integer(Integer::U128(v)))
    }

    fn visit_f32<E>(self, v: f32) -> Result<Self::Value, E> {
        Ok(Value::Float(Float::F32(v)))
    }

    fn visit_f64<E>(self, v: f64) -> Result<Self::Value, E> {
        Ok(Value::Float(Float::F64(v)))
    }

    fn visit_char<E>(self, v: char) -> Result<Self::Value, E> {
        Ok(Value::Char(v))
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E> {
        Ok(Value::String(String::from(v)))
    }

    fn visit_bytes<E>(self, v: &[u8]) -> Result<Self::Value, E> {
        Ok(Value::Bytes(Vec::from(v)))
    }

    fn visit_none<E>(self) -> Result<Self::Value, E> {
        Ok(Value::Null)
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E> {
        Ok(Value::Null)
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where
            A: SeqAccess<'de>, {
        let mut data = Vec::new();

        while let Some(elem) = seq.next_element()? {
            data.push(elem);
        }

        Ok(Value::Sequence(data))
    }

    fn visit_map<A>(self, mut map_access: A) -> Result<Self::Value, A::Error>
        where
            A: MapAccess<'de>, {
        let mut map = BTreeMap::new();

        while let Some((key, value)) = map_access.next_entry()? {
            map.insert(key, value);
        }

        Ok(Value::Map(map))
    }

    // This will only be called for capabilities, aser does not normally care about newtype structs
    fn visit_newtype_struct<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
        where
            D: Deserializer<'de>, {
        Ok(Value::Capability(CapId::deserialize(deserializer)?))
    }

    fn visit_enum<A>(self, data: A) -> Result<Self::Value, A::Error>
        where
            A: EnumAccess<'de>, {
        let (variant_index, variant_access) = data.variant()?;
        let value = Box::new(variant_access.newtype_variant()?);

        Ok(Value::EnumVariant { variant_index, value })
    }
}