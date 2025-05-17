use alloy_primitives::aliases::B32;
use serde::{Deserializer, Serializer, de::Error};
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

    if decoded.len() != 4 {
        return Err(D::Error::custom(format!(
            "expected {} bytes for array, got {}",
            32,
            decoded.len()
        )));
    }

    let mut array = [0; 4];
    array.copy_from_slice(&decoded);
    Ok(array.into())
}
