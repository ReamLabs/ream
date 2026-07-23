use alloy_primitives::{
    FixedBytes,
    hex::{self, ToHexExt},
};
use anyhow::anyhow;
use lean_multisig_leanvm::{PUB_KEY_SSZ_LEN, XmssPublicKey};
use serde::{Deserialize, Deserializer, Serialize};
use ssz::{Decode as _, Encode as _};
use ssz_derive::{Decode, Encode};
use tree_hash_derive::TreeHash;

use super::errors::LeanSigError;

pub const PUBLIC_KEY_SIZE: usize = PUB_KEY_SSZ_LEN;

#[derive(Debug, PartialEq, Clone, Encode, Decode, TreeHash, Default, Eq, Hash, Copy)]
pub struct PublicKey {
    pub inner: FixedBytes<PUBLIC_KEY_SIZE>,
}

impl From<&[u8]> for PublicKey {
    fn from(value: &[u8]) -> Self {
        Self {
            inner: FixedBytes::from_slice(value),
        }
    }
}

impl PublicKey {
    pub fn new(inner: FixedBytes<PUBLIC_KEY_SIZE>) -> Self {
        Self { inner }
    }

    pub fn from_lean_sig(public_key: &XmssPublicKey) -> Result<Self, LeanSigError> {
        Ok(Self {
            inner: FixedBytes::try_from(public_key.as_ssz_bytes().as_slice())?,
        })
    }

    pub fn as_lean_sig(&self) -> anyhow::Result<XmssPublicKey> {
        XmssPublicKey::from_ssz_bytes(self.inner.as_slice())
            .map_err(|err| anyhow!("Failed to decode XmssPublicKey from SSZ: {err:?}"))
    }
}

impl Serialize for PublicKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&format!("0x{}", self.inner.encode_hex()))
    }
}

impl<'de> Deserialize<'de> for PublicKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let result: String = Deserialize::deserialize(deserializer)?;
        let result = hex::decode(&result).map_err(serde::de::Error::custom)?;
        if result.len() != PUBLIC_KEY_SIZE {
            return Err(serde::de::Error::custom(format!(
                "Invalid public key length: {}",
                result.len()
            )));
        }
        let public_key = XmssPublicKey::from_ssz_bytes(&result)
            .map_err(|err| serde::de::Error::custom(format!("{err:?}")))?;
        Self::from_lean_sig(&public_key).map_err(serde::de::Error::custom)
    }
}
