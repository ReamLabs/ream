use std::ops::Range;

use alloy_primitives::Bytes;
use bincode::{self};
use hashsig::{
    MESSAGE_LENGTH,
    signature::{SignatureScheme, SignatureSchemeSecretKey},
};
use rand::Rng;
use serde::{Deserialize, Serialize};

use super::{BINCODE_CONFIG, errors::SignatureError};
use crate::hashsig::{HashSigScheme, public_key::PublicKey, signature::Signature};

pub type HashSigPrivateKey = <HashSigScheme as SignatureScheme>::SecretKey;

#[derive(Serialize, Deserialize)]
pub struct PrivateKey {
    inner: HashSigPrivateKey,
}

impl PrivateKey {
    pub fn new(inner: HashSigPrivateKey) -> Self {
        Self { inner }
    }

    pub fn to_bytes(&self) -> Bytes {
        bincode::serde::encode_to_vec(&self.inner, BINCODE_CONFIG)
            .expect("Failed to serialize hash sig public key")
            .into()
    }

    pub fn generate_key_pair<R: Rng>(
        rng: &mut R,
        activation_epoch: usize,
        num_active_epochs: usize,
    ) -> (PublicKey, Self) {
        let (public_key, private_key) =
            <HashSigScheme as SignatureScheme>::key_gen(rng, activation_epoch, num_active_epochs);

        (
            PublicKey::from_hash_sig_public_key(public_key),
            Self::new(private_key),
        )
    }

    /// Returns the total interval of epochs for which this key is valid.
    pub fn get_activation_interval(&self) -> Range<u64> {
        self.inner.get_activation_interval()
    }

    /// Returns the sub-interval for which the key is currently prepared to sign messages.
    pub fn get_prepared_interval(&self) -> Range<u64> {
        self.inner.get_prepared_interval()
    }

    /// Advances the prepared interval to the next one.
    ///
    /// This should be called proactively in the background as soon as half of the
    /// current prepared interval has passed.
    pub fn prepare_signature(&mut self) {
        self.inner.advance_preparation()
    }

    /// Signs a message for a given epoch.
    ///
    /// # Panics
    ///
    /// Panics if the epoch is not within the activation interval
    pub fn sign(
        &self,
        message: &[u8; MESSAGE_LENGTH],
        epoch: u32,
    ) -> anyhow::Result<Signature, SignatureError> {
        let activation_interval = self.get_activation_interval();

        assert!(
            activation_interval.contains(&(epoch as u64)),
            "Epoch {epoch} is outside the activation interval {activation_interval:?}",
        );

        let signature = <HashSigScheme as SignatureScheme>::sign(&self.inner, epoch, message)
            .map_err(SignatureError::SigningFailed)?;

        Signature::from_hash_sig_public_key(signature)
    }
}

#[cfg(test)]
mod tests {
    use rand::rng;

    use crate::hashsig::private_key::PrivateKey;

    #[test]
    fn test_sign_and_verify() {
        let mut rng = rng();
        let activation_epoch = 0;
        let num_active_epochs = 10; // Test for 10 epochs for quick key generation

        let (public_key, private_key) =
            PrivateKey::generate_key_pair(&mut rng, activation_epoch, num_active_epochs);

        let epoch = 5;

        // Create a test message (32 bytes as required by hashsig)
        let message = [0u8; 32];

        // Sign the message
        let result = private_key.sign(&message, epoch);

        assert!(result.is_ok(), "Signing should succeed");
        let signature = result.unwrap();

        // Verify the signature
        let verify_result = signature.verify(&message, &public_key, epoch);

        assert!(verify_result.is_ok(), "Verification should succeed");
        assert!(verify_result.unwrap(), "Signature should be valid");
    }

    #[test]
    #[should_panic(expected = "Epoch 100 is outside the activation interval")]
    fn test_signing_outside_activation_interval_panics() {
        let mut rng = rng();
        let activation_epoch = 5;
        let num_active_epochs = 10;

        let (_public_key, private_key) =
            PrivateKey::generate_key_pair(&mut rng, activation_epoch, num_active_epochs);

        let message = [0u8; 32];

        // Hash sig expands the interval (5, 10) to (0, 32)
        // Try to sign with an epoch outside the (expanded) activation interval (should panic)
        let _ = private_key.sign(&message, 100);
    }
}
