use alloc::collections::BTreeMap;
use alloc::{vec::Vec, boxed::Box};

use serde::{
    Serializer,
    ser::{
        SerializeTuple,
        SerializeTupleStruct,
        SerializeMap,
        SerializeSeq,
        SerializeStruct,
        SerializeTupleVariant,
        SerializeStructVariant,
    },
};
use sys::CapId;

use crate::AserError;
use crate::capability_serializer::CapabilitySerializer;
use super::{Value, Integer, Float};

pub struct ValueSerializer;

impl<'a> Serializer for ValueSerializer {
    type Ok = Value;
    type Error = AserError;

    type SerializeSeq = SequenceBuilder;
    type SerializeTuple = SequenceBuilder;
    type SerializeTupleStruct = SequenceBuilder;
    type SerializeTupleVariant = TupleVariantBuilder;
    type SerializeMap = MapBuilder;
    type SerializeStruct = MapBuilder;
    type SerializeStructVariant = StructVariantBuilder;

    fn is_human_readable(&self) -> bool {
        false
    }

    fn serialize_bool(self, v: bool) -> Result<Self::Ok, Self::Error> {
        Ok(Value::Bool(v))
    }

    fn serialize_i8(self, v: i8) -> Result<Self::Ok, Self::Error> {
        Ok(Value::Integer(Integer::I8(v)))
    }

    fn serialize_i16(self, v: i16) -> Result<Self::Ok, Self::Error> {
        Ok(Value::Integer(Integer::I16(v)))
    }

    fn serialize_i32(self, v: i32) -> Result<Self::Ok, Self::Error> {
        Ok(Value::Integer(Integer::I32(v)))
    }

    fn serialize_i64(self, v: i64) -> Result<Self::Ok, Self::Error> {
        Ok(Value::Integer(Integer::I64(v)))
    }

    fn serialize_i128(self, v: i128) -> Result<Self::Ok, Self::Error> {
        Ok(Value::Integer(Integer::I128(v)))
    }


    fn serialize_u8(self, v: u8) -> Result<Self::Ok, Self::Error> {
        Ok(Value::Integer(Integer::U8(v)))
    }

    fn serialize_u16(self, v: u16) -> Result<Self::Ok, Self::Error> {
        Ok(Value::Integer(Integer::U16(v)))
    }

    fn serialize_u32(self, v: u32) -> Result<Self::Ok, Self::Error> {
        Ok(Value::Integer(Integer::U32(v)))
    }

    fn serialize_u64(self, v: u64) -> Result<Self::Ok, Self::Error> {
        Ok(Value::Integer(Integer::U64(v)))
    }

    fn serialize_u128(self, v: u128) -> Result<Self::Ok, Self::Error> {
        Ok(Value::Integer(Integer::U128(v)))
    }

    fn serialize_f32(self, v: f32) -> Result<Self::Ok, Self::Error> {
        Ok(Value::Float(Float::F32(v)))
    }

    fn serialize_f64(self, v: f64) -> Result<Self::Ok, Self::Error> {
        Ok(Value::Float(Float::F64(v)))
    }

    fn serialize_char(self, v: char) -> Result<Self::Ok, Self::Error> {
        Ok(Value::Char(v))
    }

    fn serialize_str(self, v: &str) -> Result<Self::Ok, Self::Error> {
        Ok(Value::String(v.into()))
    }

    fn serialize_bytes(self, v: &[u8]) -> Result<Self::Ok, Self::Error> {
        Ok(Value::Bytes(v.into()))
    }

    fn serialize_unit(self) -> Result<Self::Ok, Self::Error> {
        Ok(Value::Null)
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
        Ok(Value::EnumVariant {
            variant_index,
            value: Box::new(Value::Null),
        })
    }

    fn serialize_newtype_struct<T: ?Sized>(
        self,
        _name: &'static str,
        value: &T,
    ) -> Result<Self::Ok, Self::Error>
    where
        T: serde::Serialize {
        Ok(Value::Newtype(
            Box::new(value.serialize(self)?),
        ))
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
        if variant_index == CapId::SERIALIZE_ENUM_VARIANT {
            let mut capability_serializer = CapabilitySerializer::default();
            value.serialize(&mut capability_serializer)?;

            let cap_id = CapId::try_from(capability_serializer.get_capability()? as usize)
                .ok_or(AserError::InvalidCapabilityId)?;

            Ok(Value::Capability(cap_id))
        } else {
            Ok(Value::EnumVariant {
                variant_index,
                value: Box::new(value.serialize(self)?),
            })
        }
    }

    fn serialize_seq(self, _len: Option<usize>) -> Result<Self::SerializeSeq, Self::Error> {
        Ok(SequenceBuilder::default())
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
        _len: usize,
    ) -> Result<Self::SerializeTupleVariant, Self::Error> {
        Ok(TupleVariantBuilder {
            tuple_builder: SequenceBuilder::default(),
            variant_index,
        })
    }

    fn serialize_map(self, _len: Option<usize>) -> Result<Self::SerializeMap, Self::Error> {
        Ok(MapBuilder::default())
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
        _len: usize,
    ) -> Result<Self::SerializeStructVariant, Self::Error> {
        Ok(StructVariantBuilder {
            struct_builder: MapBuilder::default(),
            variant_index,
        })
    }
}

#[derive(Default)]
pub struct SequenceBuilder(Vec<Value>);

impl SerializeSeq for SequenceBuilder {
    type Ok = Value;
    type Error = AserError;

    fn serialize_element<T: ?Sized>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: serde::Serialize {
        self.0.push(value.serialize(ValueSerializer)?);

        Ok(())
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(Value::Sequence(self.0))
    }
}

impl SerializeTuple for SequenceBuilder {
    type Ok = Value;
    type Error = AserError;

    fn serialize_element<T: ?Sized>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: serde::Serialize {
        SerializeSeq::serialize_element(self, value)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        SerializeSeq::end(self)
    }
}

impl SerializeTupleStruct for SequenceBuilder {
    type Ok = Value;
    type Error = AserError;

    fn serialize_field<T: ?Sized>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: serde::Serialize {
        SerializeSeq::serialize_element(self, value)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        SerializeSeq::end(self)
    }
}

#[derive(Default)]
pub struct MapBuilder {
    map: BTreeMap<Value, Value>,
    last_key: Option<Value>,
}

impl SerializeMap for MapBuilder {
    type Ok = Value;
    type Error = AserError;

    fn serialize_key<T: ?Sized>(&mut self, key: &T) -> Result<(), Self::Error>
    where
        T: serde::Serialize {
        self.last_key = Some(key.serialize(ValueSerializer)?);

        Ok(())
    }

    fn serialize_value<T: ?Sized>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: serde::Serialize {
        let value = value.serialize(ValueSerializer)?;

        self.map.insert(self.last_key.take().unwrap(), value);

        Ok(())
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(Value::Map(self.map))
    }
}


impl SerializeStruct for MapBuilder {
    type Ok = Value;
    type Error = AserError;

    fn serialize_field<T: ?Sized>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<(), Self::Error>
    where
        T: serde::Serialize {
        let key = Serializer::collect_str(ValueSerializer, key)?;
        let value = value.serialize(ValueSerializer)?;

        self.map.insert(key, value);

        Ok(())
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(Value::Map(self.map))
    }
}

pub struct TupleVariantBuilder {
    tuple_builder: SequenceBuilder,
    variant_index: u32,
}

impl SerializeTupleVariant for TupleVariantBuilder {
    type Ok = Value;
    type Error = AserError;

    fn serialize_field<T: ?Sized>(&mut self, value: &T) -> Result<(), Self::Error>
    where
        T: serde::Serialize {
        self.tuple_builder.serialize_field(value)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(Value::EnumVariant {
            variant_index: self.variant_index,
            value: Box::new(SerializeSeq::end(self.tuple_builder)?),
        })
    }
}

pub struct StructVariantBuilder {
    struct_builder: MapBuilder,
    variant_index: u32,
}

impl SerializeStructVariant for StructVariantBuilder {
    type Ok = Value;
    type Error = AserError;

    fn serialize_field<T: ?Sized>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<(), Self::Error>
    where
        T: serde::Serialize {
        self.struct_builder.serialize_field(key, value)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(Value::EnumVariant {
            variant_index: self.variant_index,
            value: Box::new(SerializeMap::end(self.struct_builder)?)
        })
    }
}