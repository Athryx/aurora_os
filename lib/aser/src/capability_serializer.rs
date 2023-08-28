use serde::ser::{
    Serializer,
    SerializeSeq,
    SerializeTuple,
    SerializeTupleStruct,
    SerializeTupleVariant,
    SerializeMap,
    SerializeStruct,
    SerializeStructVariant,
};

use super::AserError;

/// Used to serialize capabilties, only accepts u64s, which are capability ids
/// 
/// Once the capability is found, it will be stored in the capability_id field
#[derive(Default)]
pub struct CapabilitySerializer {
    capability_id: Option<u64>,
}

impl CapabilitySerializer {
    pub fn get_capability(&self) -> Result<u64, AserError> {
        self.capability_id.ok_or(AserError::ExpectedCapablity)
    }
}

impl Serializer for &'_ mut CapabilitySerializer {
    type Ok = ();
    type Error = AserError;

    type SerializeSeq = Self;
    type SerializeTuple = Self;
    type SerializeTupleStruct = Self;
    type SerializeTupleVariant = Self;
    type SerializeMap = Self;
    type SerializeStruct = Self;
    type SerializeStructVariant = Self;

    fn serialize_bool(self, _v: bool) -> Result<Self::Ok, Self::Error> {
        Err(AserError::ExpectedCapablity)
    }

    fn serialize_i8(self, _v: i8) -> Result<Self::Ok, Self::Error> {
        Err(AserError::ExpectedCapablity)
    }

    fn serialize_i16(self, _v: i16) -> Result<Self::Ok, Self::Error> {
        Err(AserError::ExpectedCapablity)
    }

    fn serialize_i32(self, _v: i32) -> Result<Self::Ok, Self::Error> {
        Err(AserError::ExpectedCapablity)
    }

    fn serialize_i64(self, _v: i64) -> Result<Self::Ok, Self::Error> {
        Err(AserError::ExpectedCapablity)
    }

    fn serialize_i128(self, _v:i128) -> Result<Self::Ok,Self::Error> {
        Err(AserError::ExpectedCapablity)
    }

    fn serialize_u8(self, _v: u8) -> Result<Self::Ok, Self::Error> {
        Err(AserError::ExpectedCapablity)
    }

    fn serialize_u16(self, _v: u16) -> Result<Self::Ok, Self::Error> {
        Err(AserError::ExpectedCapablity)
    }

    fn serialize_u32(self, _v: u32) -> Result<Self::Ok, Self::Error> {
        Err(AserError::ExpectedCapablity)
    }

    fn serialize_u64(self, v: u64) -> Result<Self::Ok, Self::Error> {
        if self.capability_id.is_some() {
            // cannot have already found id
            Err(AserError::MultipleCapabilties)
        } else {
            self.capability_id = Some(v);
            Ok(())
        }
    }

    fn serialize_u128(self, _v:u128) -> Result<Self::Ok,Self::Error> {
        Err(AserError::ExpectedCapablity)
    }

    fn serialize_f32(self, _v: f32) -> Result<Self::Ok, Self::Error> {
        Err(AserError::ExpectedCapablity)
    }

    fn serialize_f64(self, _v: f64) -> Result<Self::Ok, Self::Error> {
        Err(AserError::ExpectedCapablity)
    }

    fn serialize_char(self, _v: char) -> Result<Self::Ok, Self::Error> {
        Err(AserError::ExpectedCapablity)
    }

    fn serialize_str(self, _v: &str) -> Result<Self::Ok, Self::Error> {
        Err(AserError::ExpectedCapablity)
    }

    fn serialize_bytes(self, _v: &[u8]) -> Result<Self::Ok, Self::Error> {
        Err(AserError::ExpectedCapablity)
    }

    fn serialize_unit(self) -> Result<Self::Ok, Self::Error> {
        Err(AserError::ExpectedCapablity)
    }

    fn serialize_none(self) -> Result<Self::Ok, Self::Error> {
        Err(AserError::ExpectedCapablity)
    }

    fn serialize_some<T: ?Sized>(self, _value: &T) -> Result<Self::Ok, Self::Error>
    where
        T: serde::Serialize {
        Err(AserError::ExpectedCapablity)
    }

    fn serialize_unit_struct(self, _name: &'static str) -> Result<Self::Ok, Self::Error> {
        Err(AserError::ExpectedCapablity)
    }

    fn serialize_unit_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
    ) -> Result<Self::Ok, Self::Error> {
        Err(AserError::ExpectedCapablity)
    }

    fn serialize_newtype_struct<T: ?Sized>(
        self,
        _name: &'static str,
        _value: &T,
    ) -> Result<Self::Ok, Self::Error>
    where
        T: serde::Serialize {
        Err(AserError::ExpectedCapablity)
    }

    fn serialize_newtype_variant<T: ?Sized>(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        _value: &T,
    ) -> Result<Self::Ok, Self::Error>
    where
        T: serde::Serialize {
        Err(AserError::ExpectedCapablity)
    }

    fn serialize_seq(self, _len: Option<usize>) -> Result<Self::SerializeSeq, Self::Error> {
        Err(AserError::ExpectedCapablity)
    }

    fn serialize_tuple(self, _len: usize) -> Result<Self::SerializeTuple, Self::Error> {
        Err(AserError::ExpectedCapablity)
    }

    fn serialize_tuple_struct(
        self,
        _name: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeTupleStruct, Self::Error> {
        Err(AserError::ExpectedCapablity)
    }

    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeTupleVariant, Self::Error> {
        Err(AserError::ExpectedCapablity)
    }

    fn serialize_map(self, _len: Option<usize>) -> Result<Self::SerializeMap, Self::Error> {
        Err(AserError::ExpectedCapablity)
    }

    fn serialize_struct(
        self,
        _name: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeStruct, Self::Error> {
        Err(AserError::ExpectedCapablity)
    }

    fn serialize_struct_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        _variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeStructVariant, Self::Error> {
        Err(AserError::ExpectedCapablity)
    }

    fn collect_str<T: ?Sized>(self, _value: &T) -> Result<Self::Ok, Self::Error>
    where
        T: core::fmt::Display {
        Err(AserError::ExpectedCapablity)
    }
}

impl SerializeSeq for &'_ mut CapabilitySerializer {
    type Ok = ();
    type Error = AserError;

    fn serialize_element<T: ?Sized>(&mut self, _value: &T) -> Result<(), Self::Error>
    where
        T: serde::Serialize {
        Err(AserError::ExpectedCapablity)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Err(AserError::ExpectedCapablity)
    }
}

impl SerializeTuple for &'_ mut CapabilitySerializer {
    type Ok = ();
    type Error = AserError;

    fn serialize_element<T: ?Sized>(&mut self, _value: &T) -> Result<(), Self::Error>
    where
        T: serde::Serialize {
        Err(AserError::ExpectedCapablity)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Err(AserError::ExpectedCapablity)
    }
}

impl SerializeTupleStruct for &'_ mut CapabilitySerializer {
    type Ok = ();
    type Error = AserError;

    fn serialize_field<T: ?Sized>(&mut self, _value: &T) -> Result<(), Self::Error>
    where
        T: serde::Serialize {
        Err(AserError::ExpectedCapablity)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Err(AserError::ExpectedCapablity)
    }
}

impl SerializeTupleVariant for &'_ mut CapabilitySerializer {
    type Ok = ();
    type Error = AserError;

    fn serialize_field<T: ?Sized>(&mut self, _value: &T) -> Result<(), Self::Error>
    where
        T: serde::Serialize {
        Err(AserError::ExpectedCapablity)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Err(AserError::ExpectedCapablity)
    }
}

impl SerializeMap for &'_ mut CapabilitySerializer {
    type Ok = ();
    type Error = AserError;

    fn serialize_key<T: ?Sized>(&mut self, _key: &T) -> Result<(), Self::Error>
    where
        T: serde::Serialize {
        Err(AserError::ExpectedCapablity)
    }

    fn serialize_value<T: ?Sized>(&mut self, _value: &T) -> Result<(), Self::Error>
    where
        T: serde::Serialize {
        Err(AserError::ExpectedCapablity)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Err(AserError::ExpectedCapablity)
    }
}

impl SerializeStruct for &'_ mut CapabilitySerializer {
    type Ok = ();
    type Error = AserError;

    fn serialize_field<T: ?Sized>(&mut self, _key: &'static str, _value: &T) -> Result<(), Self::Error>
    where
        T: serde::Serialize {
        Err(AserError::ExpectedCapablity)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Err(AserError::ExpectedCapablity)
    }
}

impl SerializeStructVariant for &'_ mut CapabilitySerializer {
    type Ok = ();
    type Error = AserError;

    fn serialize_field<T: ?Sized>(&mut self, _key: &'static str, _value: &T) -> Result<(), Self::Error>
    where
        T: serde::Serialize {
        Err(AserError::ExpectedCapablity)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Err(AserError::ExpectedCapablity)
    }
}