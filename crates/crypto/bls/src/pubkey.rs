use std::str::FromStr;

use alloy_primitives::hex;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use ssz::Encode;
use ssz_derive::{Decode, Encode};
use ssz_types::{FixedVector, typenum::U48};
use tree_hash_derive::TreeHash;

use crate::errors::BLSError;

#[derive(Debug, PartialEq, Clone, Encode, Decode, TreeHash, Default, Eq, Hash)]
pub struct PubKey {
    pub inner: FixedVector<u8, U48>,
}

impl Serialize for PubKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let val = format!("0x{}", hex::encode(self.inner.as_ssz_bytes()));
        serializer.serialize_str(&val)
    }
}

impl<'de> Deserialize<'de> for PubKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let result: String = Deserialize::deserialize(deserializer)?;
        let result = hex::decode(&result).map_err(serde::de::Error::custom)?;
        let key = FixedVector::from(result);
        Ok(Self { inner: key })
    }
}

impl PubKey {
    pub fn to_bytes(&self) -> &[u8] {
        self.inner.iter().as_slice()
    }
}

impl FromStr for PubKey {
    type Err = BLSError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let clean_str = s.strip_prefix("0x").unwrap_or(s);
        let bytes = hex::decode(clean_str).map_err(|_| BLSError::InvalidHexString)?;

        if bytes.len() != 48 {
            return Err(BLSError::InvalidByteLength);
        }

        Ok(PubKey {
            inner: FixedVector::from(bytes),
        })
    }
}
