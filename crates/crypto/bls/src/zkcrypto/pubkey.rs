use bls12_381::{G1Affine, G1Projective};

use crate::{
    PubKey,
    errors::BLSError,
    traits::{Aggregatable, ZkcryptoAggregatable},
};

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

impl Aggregatable<PubKey> for PubKey {
    type Error = BLSError;

    fn aggregate(pubkeys: &[&PubKey]) -> Result<PubKey, Self::Error> {
        let aggregate_point = pubkeys
            .iter()
            .try_fold(G1Projective::identity(), |acc, pubkey| {
                Ok(acc.add(&G1Projective::from(G1Affine::try_from(*pubkey)?)))
            })?;

        Ok(PubKey::from(aggregate_point))
    }
}

impl ZkcryptoAggregatable<PubKey> for PubKey {}
