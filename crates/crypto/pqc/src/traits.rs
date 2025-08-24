use crate::hashsig::{Signature, public_key::PublicKey};

pub trait PQSignable {
    type Error;
    fn sign(&self, message: &[u8], epoch: u32) -> Result<Signature, Self::Error>;
}

pub trait PQVerifiable {
    type Error;
    fn verify(
        &self,
        message: &[u8],
        public_key: &PublicKey,
        epoch: u32,
    ) -> Result<bool, Self::Error>;
}
