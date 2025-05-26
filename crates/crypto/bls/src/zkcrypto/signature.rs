use bls12_381::{
    G1Affine, G2Affine, G2Projective,
    hash_to_curve::{ExpandMsgXmd, HashToCurve},
    pairing,
};

use crate::{
    BLSSignature, PubKey,
    constants::DST,
    errors::BLSError,
    traits::{Aggregatable, Verifiable, ZkcryptoAggregatable, ZkcryptoVerifiable},
};

impl TryFrom<&BLSSignature> for G2Affine {
    type Error = BLSError;

    fn try_from(value: &BLSSignature) -> Result<Self, Self::Error> {
        match G2Affine::from_compressed(
            &value
                .to_bytes()
                .try_into()
                .map_err(|_| BLSError::InvalidByteLength)?,
        )
        .into_option()
        {
            Some(point) => Ok(point),
            None => Err(BLSError::InvalidSignature),
        }
    }
}

impl From<G2Projective> for BLSSignature {
    fn from(value: G2Projective) -> Self {
        Self {
            inner: G2Affine::from(value).to_compressed().to_vec().into(),
        }
    }
}

impl Verifiable for BLSSignature {
    type Error = BLSError;

    fn verify(&self, pubkey: &PubKey, message: &[u8]) -> Result<bool, BLSError> {
        let h = <G2Projective as HashToCurve<ExpandMsgXmd<sha2::Sha256>>>::hash_to_curve(
            [message],
            DST,
        );

        let gt1 = pairing(&G1Affine::try_from(pubkey)?, &G2Affine::from(h));
        let gt2 = pairing(&G1Affine::generator(), &G2Affine::try_from(self)?);

        Ok(gt1 == gt2)
    }

    fn fast_aggregate_verify<'a, P>(&self, pubkeys: P, message: &[u8]) -> Result<bool, BLSError>
    where
        P: AsRef<[&'a PubKey]>,
    {
        let aggregate_pubkey = PubKey::aggregate(pubkeys.as_ref())?;
        let h = <G2Projective as HashToCurve<ExpandMsgXmd<sha2::Sha256>>>::hash_to_curve(
            [message],
            DST,
        );

        let gt1 = pairing(&G1Affine::try_from(&aggregate_pubkey)?, &G2Affine::from(h));
        let gt2 = pairing(&G1Affine::generator(), &G2Affine::try_from(self)?);

        Ok(gt1 == gt2)
    }
}

impl Aggregatable<BLSSignature> for BLSSignature {
    type Error = BLSError;

    fn aggregate(signatures: &[&BLSSignature]) -> Result<BLSSignature, Self::Error> {
        let aggregate_point =
            signatures
                .iter()
                .try_fold(G2Projective::identity(), |accumulator, signature| {
                    Ok(accumulator.add(&G2Projective::from(G2Affine::try_from(*signature)?)))
                })?;

        Ok(BLSSignature::from(aggregate_point))
    }
}

impl ZkcryptoAggregatable<BLSSignature> for BLSSignature {}

impl ZkcryptoVerifiable for BLSSignature {}
