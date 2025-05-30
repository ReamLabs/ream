use alloy_primitives::hex::{FromHex, ToHexExt};
use serde::{Deserialize, Deserializer, Serializer, de};

pub fn serialize<T, S>(value: &T, serializer: S) -> Result<S::Ok, S::Error>
where
    T: AsRef<[u8]>,
    S: Serializer,
{
    serializer.serialize_str(&value.encode_hex())
}

pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    Vec::<u8>::from_hex(&s).map_err(|err| de::Error::custom(err.to_string()))
}
