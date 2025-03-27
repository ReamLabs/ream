use bls12_381::{G1Affine, G1Projective};

use crate::{errors::BLSError, PubKey};

impl From<G1Projective> for PubKey {
    fn from(value: G1Projective) -> Self {
        Self {
            inner: G1Affine::from(value).to_compressed().to_vec().into(),
        }
    }
}

impl TryFrom<&PubKey> for G1Affine {
    type Error = BLSError;

    fn try_from(value: &PubKey) -> Result<Self, Self::Error> {
        match G1Affine::from_compressed(
            &value
                .to_bytes()
                .try_into()
                .map_err(|_| BLSError::InvalidByteLength)?,
        )
        .into_option()
        {
            Some(point) => Ok(point),
            None => Err(BLSError::InvalidPublicKey),
        }
    }
}
