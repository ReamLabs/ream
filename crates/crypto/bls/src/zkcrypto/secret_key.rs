use alloy_primitives::B256;
use bls12_381::{
    G2Projective, Scalar,
    hash_to_curve::{ExpandMsgXmd, HashToCurve},
};
use group::Curve;
use ssz_types::FixedVector;

use crate::{
    SecretKey,
    constants::DST,
    errors::BLSError,
    signature::BLSSignature,
    traits::{Signable, ZkcryptoSignable},
};

impl SecretKey {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, BLSError> {
        if bytes.len() != 32 {
            return Err(BLSError::InvalidByteLength);
        }

        let mut key_bytes = [0u8; 32];
        key_bytes.copy_from_slice(bytes);

        let scalar = Scalar::from_bytes(&key_bytes);
        if scalar.is_some().into() {
            Ok(Self {
                inner: B256::from_slice(&key_bytes),
            })
        } else {
            Err(BLSError::InvalidSecretKey)
        }
    }

    fn as_scalar(&self) -> Result<Scalar, BLSError> {
        let bytes = self.inner.as_slice();
        if bytes.len() != 32 {
            return Err(BLSError::InvalidByteLength);
        }

        let mut key_bytes = [0u8; 32];
        key_bytes.copy_from_slice(bytes);

        Scalar::from_bytes(&key_bytes)
            .into_option()
            .ok_or(BLSError::InvalidSecretKey)
    }
}

impl Signable for SecretKey {
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

impl ZkcryptoSignable for SecretKey {}
