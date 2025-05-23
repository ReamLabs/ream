use bls12_381::{G1Affine, G1Projective};

use crate::{
    AggregatePubKey, PubKey,
    errors::BLSError,
    traits::{Aggregatable, ZkcryptoAggregatable},
};

impl Aggregatable<PubKey> for AggregatePubKey {
    type Error = BLSError;
    type Output = AggregatePubKey;

    fn aggregate(pubkeys: &[&PubKey]) -> Result<Self::Output, Self::Error> {
        let agg_point = pubkeys
            .iter()
            .try_fold(G1Projective::identity(), |acc, pubkey| {
                Ok(acc.add(&G1Projective::from(G1Affine::try_from(*pubkey)?)))
            })?;

        Ok(Self {
            inner: PubKey::from(agg_point),
        })
    }
}

impl ZkcryptoAggregatable<PubKey, AggregatePubKey> for AggregatePubKey {}
