use alloy_primitives::B256;
use blst::min_pk::SecretKey as BlstSecretKey;
use ssz_types::FixedVector;

use crate::{
    PrivateKey,
    constants::DST,
    errors::BLSError,
    signature::BLSSignature,
    traits::{Signable, SupranationalSignable},
};

impl PrivateKey {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, BLSError> {
        let key = BlstSecretKey::key_gen(bytes, &[]).map_err(|e| BLSError::BlstError(e.into()))?;
        let key_bytes = key.to_bytes();
        Ok(Self {
            inner: B256::from_slice(&key_bytes),
        })
    }

    fn to_blst_secret_key(&self) -> Result<BlstSecretKey, BLSError> {
        let bytes = self.inner.as_slice();
        BlstSecretKey::from_bytes(bytes).map_err(|e| BLSError::BlstError(e.into()))
    }
}

impl Signable for PrivateKey {
    type Error = anyhow::Error;

    fn sign(&self, message: &[u8]) -> Result<BLSSignature, Self::Error> {
        let blst_key = self
            .to_blst_secret_key()
            .map_err(|e| anyhow::anyhow!("Failed to convert to BlstSecretKey: {}", e))?;
        let sig = blst_key.sign(message, DST, &[]);
        let bytes = sig.serialize();
        Ok(BLSSignature {
            inner: FixedVector::from(bytes.to_vec()),
        })
    }
}

impl SupranationalSignable for PrivateKey {}
