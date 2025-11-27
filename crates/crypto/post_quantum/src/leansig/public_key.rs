use alloy_primitives::{Bytes, FixedBytes, hex};
use anyhow::anyhow;
use bincode::{self};
use leansig::signature::SignatureScheme;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use ssz_derive::{Decode, Encode};
use tree_hash_derive::TreeHash;

use super::BINCODE_CONFIG;
use crate::leansig::HashSigScheme;

pub type HashSigPublicKey = <HashSigScheme as SignatureScheme>::PublicKey;

/// Wrapper around the `GeneralizedXMSSPublicKey` from the hashsig crate.
///
/// With current signature parameters, the serialized public key is 52 bytes:
/// - Public key consists of:
/// - the root of the merkle tree (an array of 8 finite field elements),
/// - a parameter for the tweakable hash (an array of 5 finite field elements).
/// - One KoalaBear finite field element is 32 bits (4 bytes).
/// - The total size is 52 bytes.
///
/// Use [FixedVector] to easily derive traits like [ssz::Encode], [ssz::Decode], and
/// [tree_hash::TreeHash], so that we can use this type in the lean state.
/// NOTE: [SignatureScheme::PublicKey] is a Rust trait that only implements [serde::Serialize] and
/// [serde::Deserialize]. So it's impossible to implement [From] or [Into] traits for it.
///
/// NOTE 2: We might use caching here (e.g., `OnceCell`) if serialization/deserialization becomes a
/// bottleneck.
#[derive(Debug, PartialEq, Clone, Encode, Decode, TreeHash, Default, Eq, Hash)]
pub struct PublicKey {
    inner: FixedBytes<52>,
}

impl From<&[u8]> for PublicKey {
    fn from(value: &[u8]) -> Self {
        Self {
            inner: FixedBytes::from_slice(value),
        }
    }
}

impl PublicKey {
    pub fn new(inner: FixedBytes<52>) -> Self {
        Self { inner }
    }

    pub fn to_bytes(&self) -> Bytes {
        self.inner.to_vec().into()
    }

    /// Create a new `PublicKey` wrapper from the original `GeneralizedXMSSPublicKey` type
    /// with serialization.
    pub fn from_hash_sig_public_key(hash_sig_public_key: HashSigPublicKey) -> Self {
        Self {
            inner: FixedBytes::from_slice(
                bincode::serde::encode_to_vec(&hash_sig_public_key, BINCODE_CONFIG)
                    .expect("Failed to serialize hash sig public key")
                    .as_slice(),
            ),
        }
    }

    /// Convert back to the original `GeneralizedXMSSPublicKey` type from the hashsig crate.
    /// To use this public key for signature verification.
    pub fn to_hash_sig_public_key(&self) -> anyhow::Result<HashSigPublicKey> {
        bincode::serde::decode_from_slice(&self.inner.0, BINCODE_CONFIG)
            .map(|(value, _)| value)
            .map_err(|err| anyhow!("Failed to decode public key: {err}"))
    }
}

impl Serialize for PublicKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let val = format!("0x{}", hex::encode(self.inner.iter().as_slice()));
        serializer.serialize_str(&val)
    }
}

impl<'de> Deserialize<'de> for PublicKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let result: String = Deserialize::deserialize(deserializer)?;
        let result = hex::decode(&result).map_err(serde::de::Error::custom)?;
        Ok(Self {
            inner: FixedBytes::from_slice(&result),
        })
    }
}
