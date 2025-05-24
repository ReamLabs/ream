use alloy_primitives::B256;
use bls12_381::{
    G2Projective, Scalar,
    hash_to_curve::{ExpandMsgXmd, HashToCurve},
};
use group::Curve;
use ssz_types::FixedVector;

use crate::{
    PrivateKey,
    constants::DST,
    errors::BLSError,
    signature::BLSSignature,
    traits::{Signable, ZkcryptoSignable},
};

impl PrivateKey {
    fn as_scalar(&self) -> Result<Scalar, BLSError> {
        Scalar::from_bytes(self.inner.as_ref())
            .into_option()
            .ok_or(BLSError::InvalidPrivateKey)
    }
}

impl Signable for PrivateKey {
    type Error = BLSError;

    fn sign(&self, message: &[u8]) -> Result<BLSSignature, Self::Error> {
        let hash_point = <G2Projective as HashToCurve<ExpandMsgXmd<sha2::Sha256>>>::hash_to_curve(
            [message],
            DST,
        );

        let scalar = self.as_scalar()?;
        let signature_point = hash_point * scalar;

        let signature_bytes = signature_point.to_affine().to_compressed();

        Ok(BLSSignature {
            inner: FixedVector::from(signature_bytes.to_vec()),
        })
    }
}

impl ZkcryptoSignable for PrivateKey {}
