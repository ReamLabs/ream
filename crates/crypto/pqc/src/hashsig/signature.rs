use hashsig::{MESSAGE_LENGTH, signature::SignatureScheme};

use crate::{
    hashsig::{private_key::HashSigScheme, public_key::PublicKey},
    traits::PQVerifiable,
};

type HashSigSignature = <HashSigScheme as SignatureScheme>::Signature;

pub struct Signature {
    pub inner: HashSigSignature,
}

impl Signature {
    pub fn new(inner: HashSigSignature) -> Self {
        Self { inner }
    }
}

impl PQVerifiable for Signature {
    fn verify(
        &self,
        message: &[u8; MESSAGE_LENGTH],
        public_key: &PublicKey,
        epoch: u32,
    ) -> anyhow::Result<bool> {
        Ok(<HashSigScheme as SignatureScheme>::verify(
            &public_key.inner,
            epoch,
            message,
            &self.inner,
        ))
    }
}
