use alloy_primitives::hex;
use bincode::{
    self,
    config::{Fixint, LittleEndian, NoLimit},
};
use hashsig::signature::SignatureScheme;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use ssz::Encode;
use ssz_derive::{Decode, Encode};
use ssz_types::{FixedVector, typenum::U52};
use tree_hash_derive::TreeHash;

use crate::hashsig::HashSigScheme;

type HashSigPublicKey = <HashSigScheme as SignatureScheme>::PublicKey;

// NOTE: `GeneralizedXMSSPublicKey` doesn't implement methods like `to_bytes`,
// which means we need to use bincode to serialize/deserialize it.
// However, using bincode's default config (little-endian + variable int encoding)
// add extra bytes to the serialized output, which is not what we want.
// Thus, define a custom configuration for bincode here.
const BINCODE_CONFIG: bincode::config::Configuration<LittleEndian, Fixint, NoLimit> =
    bincode::config::standard().with_fixed_int_encoding();

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
    inner: FixedVector<u8, U52>,
}

impl PublicKey {
    pub fn to_bytes(&self) -> &[u8] {
        self.inner.iter().as_slice()
    }

    /// Create a new `PublicKey` wrapper from the original `GeneralizedXMSSPublicKey` type
    /// with serialization.
    pub fn from_hash_sig_public_key(hash_sig_public_key: HashSigPublicKey) -> Self {
        Self {
            inner: bincode::serde::encode_to_vec(&hash_sig_public_key, BINCODE_CONFIG)
                .expect("Failed to serialize hash sig public key")
                .into(),
        }
    }

    /// Convert back to the original `GeneralizedXMSSPublicKey` type from the hashsig crate.
    /// To use this public key for signature verification.
    pub fn to_hash_sig_public_key(&self) -> anyhow::Result<HashSigPublicKey> {
        bincode::serde::decode_from_slice(&self.inner, BINCODE_CONFIG)
            .map(|(value, _)| value)
            .map_err(|err| anyhow::anyhow!("Failed to decode public key: {}", err))
    }
}

impl Serialize for PublicKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let val = format!("0x{}", hex::encode(self.inner.as_ssz_bytes()));
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
        let key = FixedVector::from(result);
        Ok(Self { inner: key })
    }
}
