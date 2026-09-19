use alloy_primitives::FixedBytes;
use anyhow::anyhow;
use leanvm_b::xmss::{Decode as _, Encode as _, SIGNATURE_SSZ_LEN, XmssSignature, verify};
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use tree_hash_derive::TreeHash;

use super::{errors::LeanSigError, public_key::PublicKey};

pub const SIGNATURE_SIZE: usize = SIGNATURE_SSZ_LEN;

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
        Self::new(FixedBytes::from([0; SIGNATURE_SIZE]))
    }

    pub fn mock() -> Self {
        use super::private_key::PrivateKey;

        let (_, private_key) = PrivateKey::generate_key_pair(0, 10);
        private_key
            .sign(&[0u8; 32], 0)
            .expect("Mock signature generation failed")
    }

    pub fn from_lean_sig(signature: &XmssSignature) -> Result<Self, LeanSigError> {
        Ok(Self {
            inner: FixedBytes::try_from(signature.as_ssz_bytes().as_slice())?,
        })
    }

    pub fn as_lean_sig(&self) -> anyhow::Result<XmssSignature> {
        XmssSignature::from_ssz_bytes(self.inner.as_slice())
            .map_err(|err| anyhow!("Failed to decode XmssSignature from SSZ: {err:?}"))
    }

    pub fn verify(
        &self,
        public_key: &PublicKey,
        epoch: u32,
        message: &[u8; 32],
    ) -> anyhow::Result<bool> {
        Ok(verify(
            &public_key.as_lean_sig()?,
            message,
            &self.as_lean_sig()?,
            epoch,
        )
        .is_ok())
    }
}

#[cfg(test)]
mod tests {
    use crate::leansig::private_key::PrivateKey;

    #[test]
    fn signature_round_trip_and_binding_checks() {
        let (public_key, private_key) = PrivateKey::generate_key_pair(0, 10);
        let message = [7u8; 32];
        let signature = private_key.sign(&message, 5).expect("signing failed");
        let recovered = super::Signature::from_lean_sig(&signature.as_lean_sig().unwrap()).unwrap();

        assert_eq!(signature, recovered);
        assert!(signature.verify(&public_key, 5, &message).unwrap());
        assert!(!signature.verify(&public_key, 5, &[8u8; 32]).unwrap());
        assert!(!signature.verify(&public_key, 6, &message).unwrap());
    }
}
