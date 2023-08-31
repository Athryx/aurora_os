use core::slice::Iter as SliceIter;
use alloc::collections::btree_map::{Keys, Values};

use serde::{
    Deserializer,
    de::{DeserializeSeed, Visitor, IntoDeserializer, SeqAccess, MapAccess, EnumAccess, VariantAccess},
    forward_to_deserialize_any,
};

use crate::AserError;
use super::{Value, Integer, Float};

impl<'de> Deserializer<'de> for &'de Value {
    type Error = AserError;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de> {
        match self {
            Value::Null => visitor.visit_unit(),
            Value::Bool(v) => visitor.visit_bool(*v),
            Value::Integer(n) => match *n {
                Integer::I8(n) => visitor.visit_i8(n),
                Integer::I16(n) => visitor.visit_i16(n),
                Integer::I32(n) => visitor.visit_i32(n),
                Integer::I64(n) => visitor.visit_i64(n),
                Integer::I128(n) => visitor.visit_i128(n),
                Integer::U8(n) => visitor.visit_u8(n),
                Integer::U16(n) => visitor.visit_u16(n),
                Integer::U32(n) => visitor.visit_u32(n),
                Integer::U64(n) => visitor.visit_u64(n),
                Integer::U128(n) => visitor.visit_u128(n),
            },
            Value::Float(n) => match *n {
                Float::F32(n) => visitor.visit_f32(n),
                Float::F64(n) => visitor.visit_f64(n),
            },
            Value::Char(c) => visitor.visit_char(*c),
            Value::String(s) => visitor.visit_borrowed_str(s),
            Value::Bytes(bytes) => visitor.visit_borrowed_bytes(bytes),
            Value::Sequence(sequence) => visitor.visit_seq(SequenceDeserializer(sequence.iter())),
            Value::Map(map) => visitor.visit_map(MapDeserializer {
                keys: map.keys(),
                values: map.values(),
            }),
            Value::Capability(cap_id) => visitor.visit_newtype_struct(usize::from(*cap_id).into_deserializer()),
            Value::EnumVariant {
                variant_index,
                value,
            } => visitor.visit_enum(EnumDeserializer {
                variant_index: *variant_index,
                value,
            }),
        }
    }

    forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
        bytes byte_buf option unit unit_struct newtype_struct seq tuple
        tuple_struct map struct enum identifier ignored_any
    }
}

struct SequenceDeserializer<'a>(SliceIter<'a, Value>);

impl<'de> SeqAccess<'de> for SequenceDeserializer<'de> {
    type Error = AserError;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, Self::Error>
        where
            T: DeserializeSeed<'de> {
        let Some(elem) = self.0.next() else {
            return Ok(None);
        };

        seed.deserialize(elem).map(Some)
    }
}

struct MapDeserializer<'a> {
    keys: Keys<'a, Value, Value>,
    values: Values<'a, Value, Value>,
}

impl<'de> MapAccess<'de> for MapDeserializer<'de> {
    type Error = AserError;

    fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>, Self::Error>
        where
            K: DeserializeSeed<'de> {
        let Some(key) = self.keys.next() else {
            return Ok(None);
        };

        seed.deserialize(key).map(Some)
    }

    fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value, Self::Error>
        where
            V: DeserializeSeed<'de> {
        seed.deserialize(self.values.next().unwrap())
    }
}

struct EnumDeserializer<'a> {
    variant_index: u32,
    value: &'a Value,
}

impl<'de> EnumAccess<'de> for EnumDeserializer<'de> {
    type Error = AserError;
    type Variant = Self;

    fn variant_seed<V>(self, seed: V) -> Result<(V::Value, Self::Variant), Self::Error>
        where
            V: DeserializeSeed<'de> {
        Ok((
            seed.deserialize(self.variant_index.into_deserializer())?,
            self,
        ))
    }
}

impl<'de> VariantAccess<'de> for EnumDeserializer<'de> {
    type Error = AserError;

    fn unit_variant(self) -> Result<(), Self::Error> {
        if self.value == &Value::Null {
            Ok(())
        } else {
            Err(AserError::EnumUnexpectedData)
        }
    }

    fn newtype_variant_seed<T>(self, seed: T) -> Result<T::Value, Self::Error>
        where
            T: DeserializeSeed<'de> {
        seed.deserialize(self.value)
    }

    fn tuple_variant<V>(self, _len: usize, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de> {
        self.value.deserialize_seq(visitor)
    }

    fn struct_variant<V>(
            self,
            _fields: &'static [&'static str],
            visitor: V,
        ) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de> {
        self.value.deserialize_map(visitor)
    }
}