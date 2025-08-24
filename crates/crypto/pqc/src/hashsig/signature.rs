use hashsig::{MESSAGE_LENGTH, signature::SignatureScheme};

use crate::{
    hashsig::{errors::VerificationError, private_key::HashSigScheme, public_key::PublicKey},
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
    type Error = VerificationError;

    fn verify(
        &self,
        message: &[u8],
        public_key: &PublicKey,
        epoch: u32,
    ) -> Result<bool, Self::Error> {
        if message.len() != MESSAGE_LENGTH {
            return Err(VerificationError::InvalidMessageLength(message.len()));
        }

        Ok(<HashSigScheme as SignatureScheme>::verify(
            &public_key.inner,
            epoch,
            &message.try_into()?,
            &self.inner,
        ))
    }
}
