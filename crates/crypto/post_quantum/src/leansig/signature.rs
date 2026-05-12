use alloy_primitives::FixedBytes;
use anyhow::anyhow;
use leansig::{MESSAGE_LENGTH, serialization::Serializable, signature::SignatureScheme};
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use tree_hash_derive::TreeHash;

use crate::leansig::{LeanSigScheme, errors::LeanSigError, public_key::PublicKey};

pub const SIGNATURE_SIZE: usize = 2536;
const SIGNATURE_PATH_OFFSET: u32 = 36;
const SIGNATURE_HASHES_OFFSET: u32 = 1064;
const SIGNATURE_PATH_SIBLINGS_OFFSET: u32 = 4;

type LeanSigSignature = <LeanSigScheme as SignatureScheme>::Signature;

/// Wrapper around a fixed-size serialized hash-based signature.
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash, Copy)]
pub struct Signature {
    pub inner: FixedBytes<SIGNATURE_SIZE>,
}

impl From<&[u8]> for Signature {
    fn from(value: &[u8]) -> Self {
        Self {
            inner: FixedBytes::from_slice(value),
        }
    }
}

impl Signature {
    pub fn new(inner: FixedBytes<SIGNATURE_SIZE>) -> Self {
        Self { inner }
    }

    pub fn blank() -> Self {
        let mut inner = [0; SIGNATURE_SIZE];

        // Structurally valid zero-valued XMSS Signature SSZ:
        // Signature { path: HashTreeOpening { siblings }, rho, hashes }.
        // Parent containers treat this as an opaque fixed-size byte blob, while
        // leanSpec can still decode it as a prod Signature when validating API output.
        inner[..4].copy_from_slice(&SIGNATURE_PATH_OFFSET.to_le_bytes());
        inner[32..36].copy_from_slice(&SIGNATURE_HASHES_OFFSET.to_le_bytes());
        inner[36..40].copy_from_slice(&SIGNATURE_PATH_SIBLINGS_OFFSET.to_le_bytes());

        Self::new(FixedBytes::from(inner))
    }

    /// Create a mock signature for testing purposes.
    pub fn mock() -> Self {
        use crate::leansig::private_key::PrivateKey;

        let (_, private_key) = PrivateKey::generate_key_pair(0, 10);
        let message = [0u8; 32];
        private_key
            .sign(&message, 0)
            .expect("Mock signature generation failed")
    }

    pub fn from_lean_sig(signature: LeanSigSignature) -> Result<Self, LeanSigError> {
        Ok(Self {
            inner: FixedBytes::try_from(signature.to_bytes().as_slice())?,
        })
    }

    pub fn as_lean_sig(&self) -> anyhow::Result<LeanSigSignature> {
        LeanSigSignature::from_bytes(self.inner.as_slice())
            .map_err(|err| anyhow!("Failed to decode LeanSigSignature from SSZ: {err:?}"))
    }

    pub fn verify(
        &self,
        public_key: &PublicKey,
        epoch: u32,
        message: &[u8; MESSAGE_LENGTH],
    ) -> anyhow::Result<bool> {
        Ok(<LeanSigScheme as SignatureScheme>::verify(
            &public_key.as_lean_sig()?,
            epoch,
            message,
            &self.as_lean_sig()?,
        ))
    }
}

#[cfg(test)]
mod tests {
    use crate::leansig::{private_key::PrivateKey, signature::Signature};

    #[test]
    fn test_serialization_roundtrip() {
        let activation_epoch = 0;
        let num_active_epochs = 10; // Test for 10 epochs for quick key generation

        let (_, private_key) = PrivateKey::generate_key_pair(activation_epoch, num_active_epochs);

        let epoch = 5;

        // Create a test message (32 bytes as required by leansig)
        let message = [0u8; 32];

        // Sign the message
        let result = private_key.sign(&message, epoch);

        assert!(result.is_ok(), "Signing should succeed");
        let signature = result.unwrap();

        // convert to leansig signature
        let hash_sig_signature = signature.as_lean_sig().unwrap();

        // convert back to signature
        let signature_returned = Signature::from_lean_sig(hash_sig_signature).unwrap();

        // verify roundtrip
        assert_eq!(signature, signature_returned);
    }

    #[test]
    fn test_blank_signature_has_valid_ssz_offsets() {
        let signature = Signature::blank();

        assert_eq!(&signature.inner[..4], 36u32.to_le_bytes().as_slice());
        assert_eq!(&signature.inner[32..36], 1064u32.to_le_bytes().as_slice());
        assert_eq!(&signature.inner[36..40], 4u32.to_le_bytes().as_slice());
    }
}
