use serde::{de::{self, Visitor, SeqAccess, MapAccess, EnumAccess, VariantAccess, IntoDeserializer}, forward_to_deserialize_any, Deserialize};

use super::capability_deserializer::CapabilityDeserializer;
use super::{AserError, DataType};

pub fn from_bytes<'a, T: Deserialize<'a>>(bytes: &'a [u8]) -> Result<T, AserError> {
    let mut deserializer = Deserializer::from_bytes(bytes)?;
    let out = T::deserialize(&mut deserializer)?;

    if deserializer.input.is_empty() {
        Ok(out)
    } else {
        Err(AserError::TrailingInput)
    }
}

pub struct Deserializer<'de> {
    capabilities: &'de [u64],
    input: &'de [u8],
}

impl<'de> Deserializer<'de> {
    pub fn from_bytes(mut data: &'de [u8]) -> Result<Deserializer<'de>, AserError> {
        let num_capabilities = data.take(..8)
            .ok_or(AserError::EndOfInput)?;

        let num_capabilities = usize::from_le_bytes(num_capabilities.try_into().unwrap());

        let capabilities = data.take(..8 * num_capabilities)
            .ok_or(AserError::EndOfInput)?;

        let capabilities = unsafe {
            core::slice::from_raw_parts(capabilities.as_ptr() as *const u64, capabilities.len() / 8)
        };

        Ok(Deserializer {
            capabilities,
            input: data,
        })
    }

    fn take_u8(&mut self) -> Result<u8, AserError> {
        self.input.take_first().copied().ok_or(AserError::EndOfInput)
    }

    fn take_u16(&mut self) -> Result<u16, AserError> {
        let bytes = self.input.take(..2).ok_or(AserError::EndOfInput)?;

        Ok(u16::from_le_bytes(bytes.try_into().unwrap()))
    }

    fn take_u32(&mut self) -> Result<u32, AserError> {
        let bytes = self.input.take(..4).ok_or(AserError::EndOfInput)?;

        Ok(u32::from_le_bytes(bytes.try_into().unwrap()))
    }

    fn take_u64(&mut self) -> Result<u64, AserError> {
        let bytes = self.input.take(..8).ok_or(AserError::EndOfInput)?;

        Ok(u64::from_le_bytes(bytes.try_into().unwrap()))
    }

    fn take_u128(&mut self) -> Result<u128, AserError> {
        let bytes = self.input.take(..16).ok_or(AserError::EndOfInput)?;

        Ok(u128::from_le_bytes(bytes.try_into().unwrap()))
    }

    fn take_data_type(&mut self) -> Result<DataType, AserError> {
        let byte = self.take_u8()?;

        DataType::try_from(byte).or(Err(AserError::InvalidDataType))
    }

    fn peek_data_type(&self) -> Result<DataType, AserError> {
        let byte = self.input.first().ok_or(AserError::EndOfInput)?;

        DataType::try_from(*byte).or(Err(AserError::InvalidDataType))
    }

    fn take_bytes(&mut self, num_bytes: usize) -> Result<&'de [u8], AserError> {
        self.input.take(..num_bytes).ok_or(AserError::EndOfInput)
    }

    fn take_str(&mut self, num_bytes: usize) -> Result<&'de str, AserError> {
        let bytes = self.take_bytes(num_bytes)?;

        core::str::from_utf8(bytes).or(Err(AserError::InvalidUtf8))
    }
}

impl<'de, 'a> de::Deserializer<'de> for &'a mut Deserializer<'de> {
    type Error = AserError;

    fn is_human_readable(&self) -> bool {
        false
    }

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de> {
        while let DataType::Filler = self.peek_data_type()? {
            self.take_data_type()?;
        }

        match self.take_data_type()? {
            DataType::Filler => panic!("unexpected filler"),

            DataType::Null => visitor.visit_unit(),

            DataType::True => visitor.visit_bool(true),
            DataType::False => visitor.visit_bool(false),

            DataType::I8 => visitor.visit_i8(self.take_u8()? as i8),
            DataType::I16 => visitor.visit_i16(self.take_u16()? as i16),
            DataType::I32 => visitor.visit_i32(self.take_u32()? as i32),
            DataType::I64 => visitor.visit_i64(self.take_u64()? as i64),
            DataType::I128 => visitor.visit_i128(self.take_u128()? as i128),

            DataType::U8 => visitor.visit_u8(self.take_u8()?),
            DataType::U16 => visitor.visit_u16(self.take_u16()?),
            DataType::U32 => visitor.visit_u32(self.take_u32()?),
            DataType::U64 => visitor.visit_u64(self.take_u64()?),
            DataType::U128 => visitor.visit_u128(self.take_u128()?),

            DataType::F32 => visitor.visit_f32(f32::from_bits(self.take_u32()?)),
            DataType::F64 => visitor.visit_f64(f64::from_bits(self.take_u64()?)),

            DataType::Char => {
                let c = char::try_from(self.take_u32()?)
                    .or(Err(AserError::InvalidUtf8))?;

                visitor.visit_char(c)
            },

            DataType::String8 => {
                let num_bytes = self.take_u8()? as usize;
                visitor.visit_borrowed_str(self.take_str(num_bytes)?)
            },
            DataType::String16 => {
                let num_bytes = self.take_u16()? as usize;
                visitor.visit_borrowed_str(self.take_str(num_bytes)?)
            },
            DataType::String32 => {
                let num_bytes = self.take_u32()? as usize;
                visitor.visit_borrowed_str(self.take_str(num_bytes)?)
            },
            DataType::String64 => {
                let num_bytes = self.take_u64()? as usize;
                visitor.visit_borrowed_str(self.take_str(num_bytes)?)
            },

            DataType::Bytes8 => {
                let num_bytes = self.take_u8()? as usize;
                visitor.visit_borrowed_bytes(self.take_bytes(num_bytes)?)
            },
            DataType::Bytes16 => {
                let num_bytes = self.take_u16()? as usize;
                visitor.visit_borrowed_bytes(self.take_bytes(num_bytes)?)
            },
            DataType::Bytes32 => {
                let num_bytes = self.take_u32()? as usize;
                visitor.visit_borrowed_bytes(self.take_bytes(num_bytes)?)
            },
            DataType::Bytes64 => {
                let num_bytes = self.take_u64()? as usize;
                visitor.visit_borrowed_bytes(self.take_bytes(num_bytes)?)
            },

            DataType::Newtype => visitor.visit_newtype_struct(self),
            DataType::Some => visitor.visit_some(self),

            DataType::SequenceStart => visitor.visit_seq(SequenceDeserializer::try_from(self)?),
            DataType::SequenceEnd => Err(AserError::UnexpectedTerminator),

            DataType::MapStart => visitor.visit_map(MapDeserializer::try_from(self)?),
            DataType::MapEnd => Err(AserError::UnexpectedTerminator),

            DataType::Variant => visitor.visit_enum(EnumDeserializer {
                deserializer: self,
                has_data: false,
            }),
            DataType::VariantValue => visitor.visit_enum(EnumDeserializer {
                deserializer: self,
                has_data: true,
            }),

            DataType::Capability => {
                let index = self.take_u16()?;
                let value = *self.capabilities.get(index as usize)
                    .ok_or(AserError::InvalidCapabilityIndex)?;

                let cap_deserializer = CapabilityDeserializer {
                    cap_id: value,
                };

                visitor.visit_enum(cap_deserializer)
            },
        }
    }

    forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
        bytes byte_buf option unit unit_struct newtype_struct seq tuple
        tuple_struct map struct enum identifier ignored_any
    }
}

struct SequenceDeserializer<'a, 'de: 'a> {
    deserializer: &'a mut Deserializer<'de>,
    finished: bool,
}

impl SequenceDeserializer<'_, '_> {
    fn check_if_finished(&mut self) -> Result<(), AserError> {
        if self.deserializer.peek_data_type()? == DataType::SequenceEnd {
            // take end byte
            self.deserializer.take_data_type().unwrap();
            self.finished = true;
        }

        Ok(())
    }
}

impl<'a, 'de: 'a> TryFrom<&'a mut Deserializer<'de>> for SequenceDeserializer<'a, 'de> {
    type Error = AserError;

    fn try_from(deserializer: &'a mut Deserializer<'de>) -> Result<Self, Self::Error> {
        let mut out = SequenceDeserializer {
            deserializer,
            finished: false,
        };

        out.check_if_finished()?;

        Ok(out)
    }
}

impl<'a, 'de> SeqAccess<'de> for SequenceDeserializer<'a, 'de> {
    type Error = AserError;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, Self::Error>
    where
        T: de::DeserializeSeed<'de> {
        if self.finished {
            return Ok(None);
        }

        let out = seed.deserialize(&mut *self.deserializer).map(Some);

        self.check_if_finished()?;

        out
    }
}

struct MapDeserializer<'a, 'de: 'a> {
    deserializer: &'a mut Deserializer<'de>,
    finished: bool,
}

impl MapDeserializer<'_, '_> {
    fn check_if_finished(&mut self) -> Result<(), AserError> {
        if self.deserializer.peek_data_type()? == DataType::MapEnd {
            // take end byte
            self.deserializer.take_data_type().unwrap();
            self.finished = true;
        }

        Ok(())
    }
}

impl<'a, 'de: 'a> TryFrom<&'a mut Deserializer<'de>> for MapDeserializer<'a, 'de> {
    type Error = AserError;

    fn try_from(deserializer: &'a mut Deserializer<'de>) -> Result<Self, Self::Error> {
        let mut out = MapDeserializer {
            deserializer,
            finished: false,
        };

        out.check_if_finished()?;

        Ok(out)
    }
}

impl<'a, 'de> MapAccess<'de> for MapDeserializer<'a, 'de> {
    type Error = AserError;

    fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>, Self::Error>
    where
        K: de::DeserializeSeed<'de> {
        if self.finished {
            return Ok(None);
        }

        seed.deserialize(&mut *self.deserializer).map(Some)
    }

    fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value, Self::Error>
    where
        V: de::DeserializeSeed<'de> {
        let out = seed.deserialize(&mut *self.deserializer);

        self.check_if_finished()?;

        out
    }
}

struct EnumDeserializer<'a, 'de: 'a> {
    deserializer: &'a mut Deserializer<'de>,
    // will be false if this EnumDeserializer was made for a Variant with no value
    has_data: bool,
}

impl<'a, 'de> EnumAccess<'de> for EnumDeserializer<'a, 'de> {
    type Error = AserError;
    type Variant = Self;

    fn variant_seed<V>(self, seed: V) -> Result<(V::Value, Self::Variant), Self::Error>
    where
        V: de::DeserializeSeed<'de> {
        let val = self.deserializer.take_u32()?;

        Ok((seed.deserialize(val.into_deserializer())?, self))
    }
}

impl<'a, 'de> VariantAccess<'de> for EnumDeserializer<'a, 'de> {
    type Error = AserError;

    fn unit_variant(self) -> Result<(), Self::Error> {
        if self.has_data {
            Err(AserError::EnumUnexpectedData)
        } else {
            Ok(())
        }
    }

    fn newtype_variant_seed<T>(self, seed: T) -> Result<T::Value, Self::Error>
    where
        T: de::DeserializeSeed<'de> {
        if self.has_data {
            seed.deserialize(self.deserializer)
        } else {
            seed.deserialize(().into_deserializer())
        }
    }

    fn tuple_variant<V>(self, _len: usize, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de> {
        if self.has_data {
            de::Deserializer::deserialize_seq(self.deserializer, visitor)
        } else {
            Err(AserError::EnumUnexpectedData)
        }
    }

    fn struct_variant<V>(
        self,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de> {
        if self.has_data {
            de::Deserializer::deserialize_map(self.deserializer, visitor)
        } else {
            Err(AserError::EnumUnexpectedData)
        }
    }
}