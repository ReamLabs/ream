use alloy_primitives::hex;
use anyhow;
use blst::min_pk::Signature;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use ssz::{Decode, Encode};
use tree_hash::{merkle_root, Hash256, PackedEncoding, TreeHash, TreeHashType};

use crate::{constants::DST, PubKey};

#[derive(Debug, PartialEq, Clone)]
pub struct BlsSignature {
    pub inner: [u8; 96],
}

impl Encode for BlsSignature {
    fn is_ssz_fixed_len() -> bool {
        true
    }
    fn ssz_append(&self, buf: &mut Vec<u8>) {
        buf.extend_from_slice(&self.inner);
    }
    fn ssz_bytes_len(&self) -> usize {
        96
    }
    fn ssz_fixed_len() -> usize {
        96
    }
}

impl Decode for BlsSignature {
    fn is_ssz_fixed_len() -> bool {
        true
    }

    fn ssz_fixed_len() -> usize {
        96
    }

    fn from_ssz_bytes(bytes: &[u8]) -> Result<Self, ssz::DecodeError> {
        let mut signature = [0u8; 96];
        signature.copy_from_slice(bytes);
        Ok(Self { inner: signature })
    }
}

impl Serialize for BlsSignature {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let val = hex::encode(self.inner);
        serializer.serialize_str(&val)
    }
}

impl<'de> Deserialize<'de> for BlsSignature {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let result: String = Deserialize::deserialize(deserializer)?;
        let result = hex::decode(&result).map_err(serde::de::Error::custom)?;
        let mut signature = [0u8; 96];
        signature.copy_from_slice(&result);
        Ok(Self { inner: signature })
    }
}

impl TreeHash for BlsSignature {
    fn tree_hash_type() -> tree_hash::TreeHashType {
        TreeHashType::Vector
    }

    fn tree_hash_packed_encoding(&self) -> PackedEncoding {
        PackedEncoding::from_vec(self.inner.to_vec())
    }

    fn tree_hash_packing_factor() -> usize {
        1
    }

    fn tree_hash_root(&self) -> Hash256 {
        merkle_root(&self.inner, 1)
    }
}

impl BlsSignature {
    fn to_blst_signature(&self) -> anyhow::Result<Signature> {
        Signature::from_bytes(&self.inner)
            .map_err(|e| anyhow::anyhow!("Failed to convert signature: {:?}", e))
    }

    /// Verifies a BLS signature against a public key and message.
    ///
    /// This function will return `false` in any error case, including:
    /// - If the signature bytes are invalid or malformed
    /// - If the public key bytes are invalid or malformed
    /// - If the actual signature verification fails
    ///
    /// # Arguments
    /// * `pubkey` - The public key to verify against
    /// * `message` - The message that was signed
    ///
    /// # Returns
    /// `bool` - true if the signature is valid, false otherwise
    pub fn verify(&self, pubkey: &PubKey, message: &[u8]) -> bool {
        self.to_blst_signature()
            .and_then(|sig| pubkey.to_blst_pubkey().map(|pk| (sig, pk)))
            .is_ok_and(|(sig, pk)| {
                sig.verify(true, message, DST, &[], &pk, false) == blst::BLST_ERROR::BLST_SUCCESS
            })
    }

    /// Verifies the signature against an aggregate of multiple public keys and a message
    ///
    /// # Arguments
    /// * `pubkeys` - Collection of public key references to be aggregated
    /// * `message` - Message that was signed
    ///
    /// # Returns
    /// * `bool` - Returns true if the signature is valid for the aggregated public keys and
    ///   message, false if the signature is invalid or if any error occurs during verification
    pub fn fast_aggregate_verify<'a, P>(&self, pubkeys: P, message: &[u8]) -> bool
    where
        P: AsRef<[&'a PubKey]>,
    {
        self.to_blst_signature()
            .and_then(|sig| {
                let public_keys = pubkeys
                    .as_ref()
                    .iter()
                    .map(|key| key.to_blst_pubkey())
                    .collect::<Result<Vec<_>, _>>()?;
                Ok((sig, public_keys))
            })
            .is_ok_and(|(sig, public_keys)| {
                sig.fast_aggregate_verify(
                    true,
                    message,
                    DST,
                    &public_keys.iter().collect::<Vec<_>>(),
                ) == blst::BLST_ERROR::BLST_SUCCESS
            })
    }
}
