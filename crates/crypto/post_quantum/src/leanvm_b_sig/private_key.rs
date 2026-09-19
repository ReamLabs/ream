use std::{fmt, fmt::Debug, ops::Range};

use anyhow::anyhow;
use leanvm_b::xmss::{XmssSecretKey, key_gen_from_seed, sign};
use rand_0_10::Rng;

use super::{errors::LeanSigError, public_key::PublicKey, signature::Signature};

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
        let end_epoch = activation_epoch
            .checked_add(num_active_epochs.saturating_sub(1))
            .expect("XMSS activation range overflow");
        let (private_key, public_key) = key_gen_from_seed(
            seed,
            activation_epoch
                .try_into()
                .expect("activation epoch exceeds u32"),
            end_epoch
                .try_into()
                .expect("activation end epoch exceeds u32"),
        )
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
        let epochs = self.inner.epoch_range();
        u64::from(*epochs.start())..u64::from(*epochs.end()) + 1
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

        let signature = sign(&self.inner, message, epoch).map_err(LeanSigError::SigningFailed)?;
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
        f.write_str("PrivateKey([REDACTED])")
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
    fn sign_verify_and_private_key_round_trip() {
        let (public_key, private_key) = PrivateKey::generate_key_pair(0, 10);
        let message = [3u8; 32];
        let signature = private_key.sign(&message, 5).expect("signing failed");
        let recovered = PrivateKey::from_bytes(&private_key.to_bytes()).unwrap();

        assert!(signature.verify(&public_key, 5, &message).unwrap());
        assert_eq!(private_key, recovered);
    }
}
