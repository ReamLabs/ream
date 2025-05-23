use blst::min_pk::SecretKey as BlstSecretKey;

use crate::{
    constants::DST,
    errors::BLSError,
    signature::BLSSignature,
    traits::{Signable, SupranationalSignable},
};

#[derive(Clone)]
pub struct SecretKey {
    pub(crate) inner: BlstSecretKey,
}

impl SecretKey {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, BLSError> {
        let key = BlstSecretKey::key_gen(bytes, &[]).map_err(|e| BLSError::BlstError(e.into()))?;
        Ok(Self { inner: key })
    }

    pub fn sign(&self, message: &[u8]) -> BLSSignature {
        let sig = self.inner.sign(message, DST, &[]);
        let bytes = sig.serialize();
        BLSSignature {
            inner: ssz_types::FixedVector::from(bytes.to_vec()),
        }
    }
}

impl Signable for SecretKey {
    type Error = BLSError;

    fn sign(&self, message: &[u8]) -> Result<BLSSignature, Self::Error> {
        Ok(self.sign(message))
    }
}

impl SupranationalSignable for SecretKey {}
