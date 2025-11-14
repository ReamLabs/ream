use alloy_primitives::FixedBytes;
use hashsig::{MESSAGE_LENGTH, signature::SignatureScheme};
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use tree_hash_derive::TreeHash;

use super::{BINCODE_CONFIG, errors::SignatureError};
use crate::hashsig::{HashSigScheme, public_key::PublicKey};

type HashSigSignature = <HashSigScheme as SignatureScheme>::Signature;

const SIGNATURE_SIZE: usize = 3100;

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
        Self::new(Default::default())
    }

    /// Create a new `Signature` wrapper from the original `GeneralizedXMSSSignature` type
    /// with serialization.
    pub fn from_hash_sig_public_key(
        hash_sig_signature: HashSigSignature,
    ) -> Result<Self, SignatureError> {
        let serialized = bincode::serde::encode_to_vec(&hash_sig_signature, BINCODE_CONFIG)
            .map_err(SignatureError::SignatureEncodeFailed)?;

        if serialized.len() > SIGNATURE_SIZE {
            return Err(SignatureError::InvalidSignatureLength);
        }
        let mut buffer = [0u8; SIGNATURE_SIZE];
        buffer[..serialized.len()].copy_from_slice(&serialized);

        Ok(Self {
            inner: FixedBytes::from(buffer),
        })
    }

    /// Convert back to the original `GeneralizedXMSSSignature` type from the hashsig crate.
    pub fn to_hash_sig_signature(&self) -> Result<HashSigSignature, SignatureError> {
        if self.inner.len() != SIGNATURE_SIZE {
            return Err(SignatureError::InvalidSignatureLength);
        }

        bincode::serde::decode_from_slice(&self.inner[..], BINCODE_CONFIG)
            .map(|(signature, _)| signature)
            .map_err(SignatureError::SignatureDecodeFailed)
    }

    pub fn verify(
        &self,
        public_key: &PublicKey,
        epoch: u32,
        message: &[u8; MESSAGE_LENGTH],
    ) -> anyhow::Result<bool> {
        Ok(<HashSigScheme as SignatureScheme>::verify(
            &public_key.to_hash_sig_public_key()?,
            epoch,
            message,
            &self.to_hash_sig_signature()?,
        ))
    }
}

#[cfg(test)]
mod tests {
    use rand::rng;

    use crate::hashsig::{private_key::PrivateKey, signature::Signature};

    #[test]
    fn test_serialization_roundtrip() {
        let mut rng = rng();
        let activation_epoch = 0;
        let num_active_epochs = 10; // Test for 10 epochs for quick key generation

        let (_, private_key) =
            PrivateKey::generate_key_pair(&mut rng, activation_epoch, num_active_epochs);

        let epoch = 5;

        // Create a test message (32 bytes as required by hashsig)
        let message = [0u8; 32];

        // Sign the message
        let result = private_key.sign(&message, epoch);

        assert!(result.is_ok(), "Signing should succeed");
        let signature = result.unwrap();

        // convert to hashsig signature
        let hash_sig_signature = signature.to_hash_sig_signature().unwrap();

        // convert back to signature
        let signature_returned = Signature::from_hash_sig_public_key(hash_sig_signature).unwrap();

        // verify roundtrip
        assert_eq!(signature, signature_returned);
    }
}
