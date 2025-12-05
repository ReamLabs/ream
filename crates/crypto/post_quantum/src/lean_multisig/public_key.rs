use alloy_primitives::{
    FixedBytes,
    hex::{self, ToHexExt},
};
use anyhow::anyhow;
use lean_multisig::{F, PrimeCharacteristicRing, XmssPublicKey};
use serde::{Deserialize, Deserializer, Serialize};
use ssz_derive::{Decode, Encode};
use tree_hash_derive::TreeHash;

use crate::lean_multisig::{
    errors::LeanMultisigError, public_key::LeanMultisigError::SerializationError,
};

/// Wrapper around the `XmssPublicKey` from lean-multisig.
///
/// The public key consists of:
/// - merkle_root: [F; 8] (8 * 4 bytes = 32 bytes, where F is KoalaBear field element)
/// - first_slot: u64 (8 bytes)
/// - log_lifetime: usize (8 bytes)
///
/// Total size: 48 bytes
#[derive(Debug, PartialEq, Clone, Encode, Decode, TreeHash, Default, Eq, Hash, Copy)]
pub struct PublicKey {
    pub inner: FixedBytes<48>,
}

impl From<&[u8]> for PublicKey {
    fn from(value: &[u8]) -> Self {
        Self {
            inner: FixedBytes::from_slice(value),
        }
    }
}

impl PublicKey {
    pub fn new(inner: FixedBytes<48>) -> Self {
        Self { inner }
    }

    pub fn from_xmss(public_key: XmssPublicKey) -> Result<Self, LeanMultisigError> {
        let mut bytes = Vec::new();

        // Serialize merkle_root (8 field elements, 4 bytes each)
        for elem in &public_key.merkle_root {
            let value_str = format!("{elem:?}");
            let value: u32 = value_str.parse().map_err(|err| {
                SerializationError(anyhow!("Failed to parse field element: {err}"))
            })?;
            bytes.extend_from_slice(&value.to_le_bytes());
        }

        // Serialize first_slot (8 bytes)
        bytes.extend_from_slice(&public_key.first_slot.to_le_bytes());

        // Serialize log_lifetime (8 bytes)
        bytes.extend_from_slice(&(public_key.log_lifetime as u64).to_le_bytes());

        Ok(Self {
            inner: FixedBytes::try_from(bytes.as_slice())
                .map_err(|err| SerializationError(anyhow!("{err:?}")))?,
        })
    }

    pub fn as_xmss(&self) -> anyhow::Result<XmssPublicKey> {
        let bytes = self.inner.as_slice();

        // Deserialize merkle_root
        let mut merkle_root = [F::from_usize(0); 8];
        for (i, chunk) in bytes[0..32].chunks(4).enumerate() {
            let value = u32::from_le_bytes(chunk.try_into()?);
            merkle_root[i] = F::from_usize(value as usize);
        }

        // Deserialize first_slot
        let first_slot = u64::from_le_bytes(bytes[32..40].try_into()?);

        // Deserialize log_lifetime
        let log_lifetime = u64::from_le_bytes(bytes[40..48].try_into()?) as usize;

        Ok(XmssPublicKey {
            merkle_root,
            first_slot,
            log_lifetime,
        })
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
        let bytes = hex::decode(&result).map_err(serde::de::Error::custom)?;

        Ok(Self {
            inner: FixedBytes::try_from(bytes.as_slice()).map_err(serde::de::Error::custom)?,
        })
    }
}
