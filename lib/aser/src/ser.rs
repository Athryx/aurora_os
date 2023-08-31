use core::fmt::Write;

use serde::{ser, Serialize};
use sys::CapId;

use crate::ByteBuf;

use super::{AserError, DataType, capability_serializer::CapabilitySerializer, count_capabilties};

pub fn to_bytes<T: Serialize, B: ByteBuf>(data: &T, num_capabilities: usize) -> Result<B, AserError> {
    let mut serializer = Serializer::new(num_capabilities);
    data.serialize(&mut serializer)?;

    let Serializer {
        buf,
        ..
    } = serializer;

    Ok(buf)
}

pub fn to_bytes_count_cap<T: Serialize, B: ByteBuf>(data: &T) -> Result<B, AserError> {
    let num_capabilities = count_capabilties(data)?;
    to_bytes(data, num_capabilities)
}

pub struct Serializer<B: ByteBuf> {
    /// The index where the next capability should be inserted
    capability_index: usize,
    /// Offset from start of array to data, beginning of array contains capabilities, and first 8 byte cap count
    data_offset: usize,
    buf: B,
}

impl<B: ByteBuf> Serializer<B> {
    pub fn new(num_capabilties: usize) -> Self {
        let mut buf = B::default();

        buf.extend_from_slice(&num_capabilties.to_le_bytes());
        for _ in 0..num_capabilties * 8 {
            buf.push(0);
        }

        Serializer {
            capability_index: 0,
            data_offset: buf.len(),
            buf,
        }
    }

    fn push_u16(&mut self, val: u16) {
        self.buf.extend_from_slice(&val.to_le_bytes());
    }

    fn push_u32(&mut self, val: u32) {
        self.buf.extend_from_slice(&val.to_le_bytes());
    }

    fn push_u64(&mut self, val: u64) {
        self.buf.extend_from_slice(&val.to_le_bytes());
    }

    fn push_u128(&mut self, val: u128) {
        self.buf.extend_from_slice(&val.to_le_bytes());
    }

    fn push_type(&mut self, data_type: DataType) {
        self.buf.push(data_type.into());
    }

    fn push_capability(&mut self, cap_id: u64) -> Result<(), AserError> {
        if self.capability_index >= self.data_offset {
            return Err(AserError::TooManyCapabilities);
        }

        let dest_slice = &mut self.buf.as_slice()[self.capability_index..self.capability_index + 8];
        dest_slice.copy_from_slice(&cap_id.to_le_bytes());

        self.capability_index += 8;

        Ok(())
    }
}

macro_rules! push_correct_size_type {
    ($self:expr, $size:expr, $t8:expr, $t16:expr, $t32:expr, $t64:expr) => {
        if $size <= u8::MAX as usize {
            $self.push_type($t8);
            $self.buf.push($size as u8);
        } else if $size <= u16::MAX as usize {
            $self.push_type($t16);
            $self.push_u16($size as u16);
        } else if $size <= u32::MAX as usize {
            $self.push_type($t32);
            $self.push_u32($size as u32);
        } else {
            $self.push_type($t64);
            $self.push_u64($size as u64);
        }
    };
}

impl<'a, B: ByteBuf> ser::Serializer for &'a mut Serializer<B> {
    type Ok = ();
    type Error = AserError;

    type SerializeSeq = Self;
    type SerializeTuple = Self;
    type SerializeTupleStruct = Self;
    type SerializeTupleVariant = Self;
    type SerializeMap = Self;
    type SerializeStruct = Self;
    type SerializeStructVariant = Self;

    fn is_human_readable(&self) -> bool {
        false
    }

    fn serialize_bool(self, v: bool) -> Result<Self::Ok, Self::Error> {
        if v {
            self.push_type(DataType::True);
        } else {
            self.push_type(DataType::False);
        }

        Ok(())
    }

    fn serialize_i8(self, v: i8) -> Result<Self::Ok, Self::Error> {
        self.push_type(DataType::I8);
        self.buf.push(v as u8);

        Ok(())
    }

    fn serialize_i16(self, v: i16) -> Result<Self::Ok, Self::Error> {
        self.push_type(DataType::I16);
        self.push_u16(v as u16);

        Ok(())
    }

    fn serialize_i32(self, v: i32) -> Result<Self::Ok, Self::Error> {
        self.push_type(DataType::I32);
        self.push_u32(v as u32);

        Ok(())
    }

    fn serialize_i64(self, v: i64) -> Result<Self::Ok, Self::Error> {
        self.push_type(DataType::I64);
        self.push_u64(v as u64);

        Ok(())
    }

    fn serialize_i128(self, v: i128) -> Result<Self::Ok, Self::Error> {
        self.push_type(DataType::I128);
        self.push_u128(v as u128);

        Ok(())
    }


    fn serialize_u8(self, v: u8) -> Result<Self::Ok, Self::Error> {
        self.push_type(DataType::U8);
        self.buf.push(v);

        Ok(())
    }

    fn serialize_u16(self, v: u16) -> Result<Self::Ok, Self::Error> {
        self.push_type(DataType::U16);
        self.push_u16(v);

        Ok(())
    }

    fn serialize_u32(self, v: u32) -> Result<Self::Ok, Self::Error> {
        self.push_type(DataType::U32);
        self.push_u32(v);

        Ok(())
    }

    fn serialize_u64(self, v: u64) -> Result<Self::Ok, Self::Error> {
        self.push_type(DataType::U64);
        self.push_u64(v);

        Ok(())
    }

    fn serialize_u128(self, v: u128) -> Result<Self::Ok, Self::Error> {
        self.push_type(DataType::U128);
        self.push_u128(v);

        Ok(())
    }

    fn serialize_f32(self, v: f32) -> Result<Self::Ok, Self::Error> {
        self.push_type(DataType::F32);
        self.push_u32(v.to_bits());

        Ok(())
    }

    fn serialize_f64(self, v: f64) -> Result<Self::Ok, Self::Error> {
        self.push_type(DataType::F64);
        self.push_u64(v.to_bits());

        Ok(())
    }

    fn serialize_char(self, v: char) -> Result<Self::Ok, Self::Error> {
        self.push_type(DataType::Char);
        self.push_u32(v as u32);

        Ok(())
    }

    fn serialize_str(self, v: &str) -> Result<Self::Ok, Self::Error> {
        let data = v.as_bytes();

        push_correct_size_type!(
            self,
            data.len(),
            DataType::String8,
            DataType::String16,
            DataType::String32,
            DataType::String64
        );

        self.buf.extend_from_slice(data);

        Ok(())
    }

    fn serialize_bytes(self, v: &[u8]) -> Result<Self::Ok, Self::Error> {
        push_correct_size_type!(
            self,
            v.len(),
            DataType::Bytes8,
            DataType::Bytes16,
            DataType::Bytes32,
            DataType::Bytes64
        );

        self.buf.extend_from_slice(v);

        Ok(())
    }

    fn serialize_unit(self) -> Result<Self::Ok, Self::Error> {
        self.push_type(DataType::Null);

        Ok(())
    }

    fn serialize_none(self) -> Result<Self::Ok, Self::Error> {
        self.serialize_unit()
    }

    fn serialize_some<T: ?Sized>(self, value: &T) -> Result<Self::Ok, Self::Error>
    where
        T: serde::Serialize {
        value.serialize(self)
    }

    fn serialize_unit_struct(self, _name: &'static str) -> Result<Self::Ok, Self::Error> {
        self.serialize_unit()
    }

    fn serialize_unit_variant(
        self,
        _name: &'static str,
        variant_index: u32,
        _variant: &'static str,
    ) -> Result<Self::Ok, Self::Error> {
        self.push_type(DataType::Variant);
        self.push_u32(variant_index);

        Ok(())
    }

    fn serialize_newtype_struct<T: ?Sized>(
        self,
        name: &'static str,
        value: &T,
    ) -> Result<Self::Ok, Self::Error>
    where
        T: serde::Serialize {
        if name == CapId::SERIALIZE_NEWTYPE_NAME {
            self.push_type(DataType::Capability);
            self.push_u16((self.capability_index / 8) as u16);

            let mut capability_serializer = CapabilitySerializer::default();
            value.serialize(&mut capability_serializer)?;
            
            self.push_capability(capability_serializer.get_capability()?)
        } else {
            value.serialize(self)
        }
    }

    fn serialize_newtype_variant<T: ?Sized>(
        self,
        _name: &'static str,
        variant_index: u32,
        _variant: &'static str,
        value: &T,
    ) -> Result<Self::Ok, Self::Error>
    where
        T: serde::Serialize {
        self.push_type(DataType::VariantValue);
        self.push_u32(variant_index);

        value.serialize(self)
    }

    fn serialize_seq(self, _len: Option<usize>) -> Result<Self::SerializeSeq, Self::Error> {
        self.push_type(DataType::SequenceStart);

        Ok(self)
    }

    fn serialize_tuple(self, len: usize) -> Result<Self::SerializeTuple, Self::Error> {
        self.serialize_seq(Some(len))
    }

    fn serialize_tuple_struct(
        self,
        _name: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleStruct, Self::Error> {
        self.serialize_seq(Some(len))
    }

    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        variant_index: u32,
        _variant: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleVariant, Self::Error> {
        self.push_type(DataType::VariantValue);
        self.push_u32(variant_index);

        self.serialize_seq(Some(len))
    }

    fn serialize_map(self, _len: Option<usize>) -> Result<Self::SerializeMap, Self::Error> {
        self.push_type(DataType::MapStart);

        Ok(self)
    }

    fn serialize_struct(
        self,
        _name: &'static str,
        len: usize,
    ) -> Result<Self::SerializeStruct, Self::Error> {
        self.serialize_map(Some(len))
    }

    fn serialize_struct_variant(
        self,
        _name: &'static str,
        variant_index: u32,
        _variant: &'static str,
        len: usize,
    ) -> Result<Self::SerializeStructVariant, Self::Error> {
        self.push_type(DataType::VariantValue);
        self.push_u32(variant_index);

        self.serialize_map(Some(len))
    }

    fn collect_str<T: ?Sized>(self, value: &T) -> Result<Self::Ok, Self::Error>
    where
        T: core::fmt::Display {
        // since we don't know the size of the string yet, use 64 byte size to write it
        self.push_type(DataType::String64);
        
        let size_index = self.buf.len();
        // for now specify size of 0
        self.push_u64(0);

        let start_write_index = self.buf.len();
        write!(self, "{}", value).or(Err(AserError::FormattingError))?;
        let end_write_index = self.buf.len();

        let write_size = end_write_index - start_write_index;

        // update write size after we know how much was written
        self.buf.as_slice()[size_index..start_write_index]
            .copy_from_slice(&write_size.to_le_bytes());

        Ok(())
    }
}

impl<'a, B: ByteBuf> Write for Serializer<B> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let data = s.as_bytes();

        self.buf.extend_from_slice(data);

        Ok(())
    }
}

impl<'a, B: ByteBuf> ser::SerializeSeq for &'a mut Serializer<B> {
    type Ok = ();
    type Error = AserError;

    fn serialize_element<T: ?Sized>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: serde::Serialize {
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        // TODO: figure out if this will always be used
        self.push_type(DataType::SequenceEnd);

        Ok(())
    }
}

impl<'a, B: ByteBuf> ser::SerializeTuple for &'a mut Serializer<B> {
    type Ok = ();
    type Error = AserError;

    fn serialize_element<T: ?Sized>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: serde::Serialize {
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        // TODO: figure out if this will always be used
        self.push_type(DataType::SequenceEnd);

        Ok(())
    }
}

impl<'a, B: ByteBuf> ser::SerializeTupleStruct for &'a mut Serializer<B> {
    type Ok = ();
    type Error = AserError;

    fn serialize_field<T: ?Sized>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: serde::Serialize {
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        // TODO: figure out if this will always be used
        self.push_type(DataType::SequenceEnd);

        Ok(())
    }
}

impl<'a, B: ByteBuf> ser::SerializeTupleVariant for &'a mut Serializer<B> {
    type Ok = ();
    type Error = AserError;

    fn serialize_field<T: ?Sized>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: serde::Serialize {
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        // TODO: figure out if this will always be used
        self.push_type(DataType::SequenceEnd);

        Ok(())
    }
}

impl<'a, B: ByteBuf> ser::SerializeMap for &'a mut Serializer<B> {
    type Ok = ();
    type Error = AserError;

    fn serialize_key<T: ?Sized>(&mut self, key: &T) -> Result<(), Self::Error>
    where
        T: serde::Serialize {
        key.serialize(&mut **self)
    }

    fn serialize_value<T: ?Sized>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: serde::Serialize {
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        // TODO: figure out if this will always be used
        self.push_type(DataType::MapEnd);

        Ok(())
    }
}

impl<'a, B: ByteBuf> ser::SerializeStruct for &'a mut Serializer<B> {
    type Ok = ();
    type Error = AserError;

    fn serialize_field<T: ?Sized>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<(), Self::Error>
    where
        T: serde::Serialize {
        ser::Serializer::collect_str(&mut **self, key)?;
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        // TODO: figure out if this will always be used
        self.push_type(DataType::MapEnd);

        Ok(())
    }
}

impl<'a, B: ByteBuf> ser::SerializeStructVariant for &'a mut Serializer<B> {
    type Ok = ();
    type Error = AserError;

    fn serialize_field<T: ?Sized>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<(), Self::Error>
    where
        T: serde::Serialize {
        ser::Serializer::collect_str(&mut **self, key)?;
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        // TODO: figure out if this will always be used
        self.push_type(DataType::MapEnd);

        Ok(())
    }
}