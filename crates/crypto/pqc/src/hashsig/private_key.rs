use hashsig::{
    MESSAGE_LENGTH,
    signature::{
        SignatureScheme,
        generalized_xmss::instantiations_poseidon::lifetime_2_to_the_18::winternitz::SIGWinternitzLifetime18W4,
    },
};

use crate::{hashsig::Signature, traits::PQSignable};

pub type HashSigScheme = SIGWinternitzLifetime18W4;
pub type HashSigPrivateKey = <HashSigScheme as SignatureScheme>::SecretKey;

pub struct PrivateKey {
    inner: HashSigPrivateKey,
}

impl PrivateKey {
    pub fn new(inner: HashSigPrivateKey) -> Self {
        Self { inner }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SigningError {
    #[error("Message length must be exactly {MESSAGE_LENGTH} bytes, got {0}")]
    InvalidMessageLength(usize),
    #[error("Signing failed: {0:?}")]
    SigningFailed(hashsig::signature::SigningError),
}

impl PQSignable for PrivateKey {
    type Error = SigningError;

    fn sign(&self, message: &[u8], epoch: u32) -> Result<Signature, Self::Error> {
        if message.len() != MESSAGE_LENGTH {
            return Err(SigningError::InvalidMessageLength(message.len()));
        }

        let message_array: [u8; MESSAGE_LENGTH] = message
            .try_into()
            .map_err(|_| SigningError::InvalidMessageLength(message.len()))?;

        Ok(Signature::new(
            <HashSigScheme as SignatureScheme>::sign(
                &mut rand::rng(),
                &self.inner,
                epoch,
                &message_array,
            )
            .map_err(SigningError::SigningFailed)?,
        ))
    }
}
