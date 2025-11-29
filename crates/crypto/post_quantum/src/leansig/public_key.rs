use alloy_primitives::FixedBytes;
use anyhow::anyhow;
use leansig::{serialization::Serializable, signature::SignatureScheme};
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use tree_hash_derive::TreeHash;

use crate::leansig::{LeanSigScheme, errors::LeanSigError};

pub type LeanSigPublicKey = <LeanSigScheme as SignatureScheme>::PublicKey;

/// Wrapper around the `GeneralizedXMSSPublicKey` from the leansig crate.
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
#[derive(
    Debug,
    PartialEq,
    Clone,
    Encode,
    Decode,
    TreeHash,
    Default,
    Eq,
    Hash,
    Copy,
    Deserialize,
    Serialize,
)]
#[serde(transparent)]
pub struct PublicKey {
    pub inner: FixedBytes<52>,
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

    pub fn from_lean_sig(public_key: LeanSigPublicKey) -> Result<Self, LeanSigError> {
        Ok(Self {
            inner: FixedBytes::try_from(public_key.to_bytes().as_slice())?,
        })
    }

    pub fn as_lean_sig(&self) -> anyhow::Result<LeanSigPublicKey> {
        LeanSigPublicKey::from_bytes(self.inner.as_slice())
            .map_err(|err| anyhow!("Failed to decode LeanSigPublicKey from SSZ: {err:?}"))
    }
}
