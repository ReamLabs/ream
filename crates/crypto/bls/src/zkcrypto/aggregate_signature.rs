use bls12_381::{G2Affine, G2Projective};

use crate::{
    AggregateSignature, BLSSignature,
    errors::BLSError,
    traits::{Aggregatable, ZkcryptoAggregatable},
};

impl Aggregatable<BLSSignature> for AggregateSignature {
    type Error = BLSError;
    type Output = AggregateSignature;

    fn aggregate(signatures: &[&BLSSignature]) -> Result<Self::Output, Self::Error> {
        let agg_point = signatures
            .iter()
            .try_fold(G2Projective::identity(), |acc, signature| {
                Ok(acc.add(&G2Projective::from(G2Affine::try_from(*signature)?)))
            })?;

        Ok(Self {
            inner: BLSSignature::from(agg_point),
        })
    }
}

impl ZkcryptoAggregatable<BLSSignature, AggregateSignature> for AggregateSignature {}
