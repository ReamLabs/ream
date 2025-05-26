use alloy_primitives::aliases::B32;
use serde::{Deserializer, Serializer};
use serde_utils::hex::{self, PrefixedHexVisitor};

pub fn serialize<S>(hash: &B32, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(&format!("0x{}", hex::encode(hash)))
}

pub fn deserialize<'de, D>(deserializer: D) -> Result<B32, D::Error>
where
    D: Deserializer<'de>,
{
    let decoded = deserializer.deserialize_str(PrefixedHexVisitor)?;
    B32::try_from(decoded.as_slice()).map_err(serde::de::Error::custom)
}
