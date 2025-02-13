use alloy_primitives::hex;
use blst::min_pk::PublicKey;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use ssz::Encode;
use ssz_derive::{Decode, Encode};
use ssz_types::{typenum, FixedVector};
use tree_hash_derive::TreeHash;

#[derive(Debug, PartialEq, Clone, Encode, Decode, TreeHash, Default)]
pub struct PubKey {
    pub inner: FixedVector<u8, typenum::U48>,
}

impl From<PublicKey> for PubKey {
    fn from(value: PublicKey) -> Self {
        PubKey {
            inner: FixedVector::from(value.to_bytes().to_vec()),
        }
    }
}

impl Serialize for PubKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let val = hex::encode(self.inner.as_ssz_bytes());
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
