use num_enum::TryFromPrimitive;
use serde::Serialize;

use crate::DataType;

pub enum ElemType {
    Null,
    Bool,
    Integer,
    Float,
    Char,
    String,
    Bytes,
    Sequence,
    Map,
}

pub struct UnserializedData<'a> {
    elem_type: ElemType,
    data: &'a [u8],
}

impl Serialize for UnserializedData<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self.elem_type {
            ElemType::Null => serializer.serialize_none(),
            ElemType::Bool => serializer.serialize_bool(*v),
            ElemType::Integer => n.serialize(serializer),
            ElemType::Float(n) => n.serialize(serializer),
            ElemType::Char(c) => serializer.serialize_char(*c),
            ElemType::String(s) => serializer.serialize_str(&s),
            ElemType::Bytes(data) => serializer.serialize_bytes(&data),
            ElemType::Sequence(data) => {
                let mut seq_serializer = serializer.serialize_tuple(data.len())?;

                for value in data {
                    seq_serializer.serialize_element(value)?;
                }

                seq_serializer.end()
            },
            ElemType::Map(map) => {
                let mut map_serializer = serializer.serialize_map(Some(map.len()))?;

                for (key, value) in map.iter() {
                    map_serializer.serialize_key(key)?;
                    map_serializer.serialize_value(value)?;
                }

                map_serializer.end()
            },
            ElemType::Capability(cap_id) => serializer.serialize_newtype_struct(
                CapId::SERIALIZE_NEWTYPE_NAME,
                &usize::from(*cap_id)
            ),
            ElemType::EnumVariant {
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