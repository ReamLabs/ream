use hashsig::MESSAGE_LENGTH;
use rand::Rng;

use crate::hashsig::{public_key::PublicKey, signature::Signature};

pub trait PQSignable {
    type Error;

    fn sign<R: Rng>(
        &self,
        rng: &mut R,
        message: &[u8; MESSAGE_LENGTH],
        epoch: u32,
    ) -> anyhow::Result<Signature, Self::Error>;
}

pub trait PQVerifiable {
    fn verify(
        &self,
        message: &[u8; MESSAGE_LENGTH],
        public_key: &PublicKey,
        epoch: u32,
    ) -> anyhow::Result<bool>;
}
