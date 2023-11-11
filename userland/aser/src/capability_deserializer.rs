use serde::de::{self, EnumAccess, VariantAccess, Visitor, IntoDeserializer};
use sys::CapId;

use super::AserError;

pub struct CapabilityDeserializer {
    pub cap_id: u64,
}

impl<'de> EnumAccess<'de> for CapabilityDeserializer {
    type Error = AserError;
    type Variant = Self;

    fn variant_seed<V>(self, seed: V) -> Result<(V::Value, Self::Variant), Self::Error>
    where
        V: de::DeserializeSeed<'de> {
        let varient_index = CapId::SERIALIZE_ENUM_VARIANT.into_deserializer();
        Ok((seed.deserialize(varient_index)?, self))
    }
}

impl<'de> VariantAccess<'de> for CapabilityDeserializer {
    type Error = AserError;

    fn unit_variant(self) -> Result<(), Self::Error> {
        Err(AserError::ExpectedCapablity)
    }

    fn newtype_variant_seed<T>(self, seed: T) -> Result<T::Value, Self::Error>
    where
        T: de::DeserializeSeed<'de> {
        seed.deserialize(self.cap_id.into_deserializer())
    }

    fn tuple_variant<V>(self, _len: usize, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de> {
        Err(AserError::ExpectedCapablity)
    }

    fn struct_variant<V>(
        self,
        _fields: &'static [&'static str],
        _visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de> {
        Err(AserError::ExpectedCapablity)
    }
}