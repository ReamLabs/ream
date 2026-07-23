use std::{fmt, fmt::Debug, ops::Range};

use alloy_primitives::hex::ToHexExt;
use anyhow::anyhow;
use lean_multisig_leanvm::{XmssSecretKey, xmss_key_gen_from_seed, xmss_sign};
use rand_0_10::Rng;

use super::errors::LeanSigError;
use super::{public_key::PublicKey, signature::Signature};

pub type LeanSigPrivateKey = XmssSecretKey;

pub struct PrivateKey {
    pub inner: XmssSecretKey,
}

impl PrivateKey {
    pub fn new(inner: XmssSecretKey) -> Self {
        Self { inner }
    }

    pub fn generate_key_pair_from_seed(
        seed: [u8; 32],
        activation_epoch: usize,
        num_active_epochs: usize,
    ) -> (PublicKey, Self) {
        let (public_key, private_key) =
            xmss_key_gen_from_seed(seed, activation_epoch as u64, num_active_epochs as u64)
                .expect("XMSS key generation failed: invalid activation range");

        (
            PublicKey::from_lean_sig(&public_key)
                .expect("We are generating this internally so it shouldn't fail"),
            Self::new(private_key),
        )
    }

    pub fn generate_key_pair(
        activation_epoch: usize,
        num_active_epochs: usize,
    ) -> (PublicKey, Self) {
        let mut seed = [0u8; 32];
        rand_0_10::rng().fill_bytes(&mut seed);
        Self::generate_key_pair_from_seed(seed, activation_epoch, num_active_epochs)
    }

    pub fn get_activation_interval(&self) -> Range<u64> {
        let slots = self.inner.activation_slots();
        u64::from(*slots.start())..u64::from(*slots.end()) + 1
    }

    pub fn get_prepared_interval(&self) -> Range<u64> {
        self.get_activation_interval()
    }

    pub fn prepare_signature(&mut self) {}

    pub fn prepare_epoch(&self, epoch: u32) -> Result<(), LeanSigError> {
        self.inner
            .prepare(epoch)
            .map_err(LeanSigError::SigningFailed)
    }

    pub fn sign(&self, message: &[u8; 32], epoch: u32) -> Result<Signature, LeanSigError> {
        let activation_interval = self.get_activation_interval();

        assert!(
            activation_interval.contains(&u64::from(epoch)),
            "Epoch {epoch} is outside the activation interval {activation_interval:?}",
        );

        let signature =
            xmss_sign(&self.inner, epoch, message).map_err(LeanSigError::SigningFailed)?;

        Signature::from_lean_sig(&signature)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, LeanSigError> {
        Ok(Self {
            inner: postcard::from_bytes(bytes)
                .map_err(|err| LeanSigError::DeserializationError(anyhow!("{err:?}")))?,
        })
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        postcard::to_allocvec(&self.inner).expect("XMSS secret key serialization failed")
    }
}

impl Debug for PrivateKey {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.to_bytes().encode_hex())
    }
}

impl PartialEq for PrivateKey {
    fn eq(&self, other: &Self) -> bool {
        self.to_bytes() == other.to_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::PrivateKey;

    #[test]
    fn test_sign_and_verify() {
        let (public_key, private_key) = PrivateKey::generate_key_pair(0, 10);

        let epoch = 5;
        let message = [0u8; 32];

        let signature = private_key
            .sign(&message, epoch)
            .expect("Signing should succeed");

        let verify_result = signature.verify(&public_key, epoch, &message);
        assert!(verify_result.is_ok(), "Verification should succeed");
        assert!(verify_result.unwrap(), "Signature should be valid");
    }

    #[test]
    fn test_private_key_roundtrip() {
        let (_, private_key) = PrivateKey::generate_key_pair(0, 10);
        let recovered = PrivateKey::from_bytes(&private_key.to_bytes()).unwrap();
        assert_eq!(private_key, recovered);
    }
}
