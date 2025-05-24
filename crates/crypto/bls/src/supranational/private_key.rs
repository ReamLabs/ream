use anyhow::anyhow;
use blst::min_pk::SecretKey as BlstSecretKey;
use ssz_types::FixedVector;

use crate::{
    PrivateKey,
    constants::DST,
    signature::BLSSignature,
    traits::{Signable, SupranationalSignable},
};

impl Signable for PrivateKey {
    type Error = anyhow::Error;

    fn sign(&self, message: &[u8]) -> Result<BLSSignature, Self::Error> {
        let private_key = BlstSecretKey::from_bytes(self.inner.as_slice())
            .map_err(|err| anyhow!("Failed to convert to BlstSecretKey: {err:?}"))?;
        let signature = private_key.sign(message, DST, &[]);
        Ok(BLSSignature {
            inner: FixedVector::new(signature.serialize().to_vec())
                .map_err(|err| anyhow!("Failed to create to BLSSignature: {err:?}"))?,
        })
    }
}

impl SupranationalSignable for PrivateKey {}
