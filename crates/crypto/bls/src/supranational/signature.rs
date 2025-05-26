use anyhow::anyhow;
use blst::{
    BLST_ERROR,
    min_pk::{AggregateSignature as BlstAggregateSignature, Signature as BlstSignature},
};
use ssz_types::FixedVector;

use crate::{
    constants::DST,
    errors::BLSError,
    pubkey::PubKey,
    signature::BLSSignature,
    traits::{Aggregatable, SupranationalAggregatable, SupranationalVerifiable, Verifiable},
};

impl BLSSignature {
    pub fn to_blst_signature(&self) -> Result<BlstSignature, BLSError> {
        BlstSignature::from_bytes(&self.inner).map_err(|e| BLSError::BlstError(e.into()))
    }
}

impl TryFrom<BlstSignature> for BLSSignature {
    type Error = BLSError;

    fn try_from(value: BlstSignature) -> Result<Self, Self::Error> {
        Ok(BLSSignature {
            inner: FixedVector::new(value.to_bytes().to_vec())
                .map_err(|_| BLSError::InvalidSignature)?,
        })
    }
}

impl Verifiable for BLSSignature {
    type Error = BLSError;

    fn verify(&self, pubkey: &PubKey, message: &[u8]) -> Result<bool, BLSError> {
        let signature = self.to_blst_signature()?;
        let public_key = pubkey.to_blst_pubkey()?;

        Ok(
            signature.verify(true, message, DST, &[], &public_key, false)
                == BLST_ERROR::BLST_SUCCESS,
        )
    }

    fn fast_aggregate_verify<'a, P>(&self, pubkeys: P, message: &[u8]) -> Result<bool, BLSError>
    where
        P: AsRef<[&'a PubKey]>,
    {
        let signature = self.to_blst_signature()?;
        let public_keys = pubkeys
            .as_ref()
            .iter()
            .map(|key| key.to_blst_pubkey())
            .collect::<Result<Vec<_>, _>>()?;

        Ok(signature.fast_aggregate_verify(
            true,
            message,
            DST,
            &public_keys.iter().collect::<Vec<_>>(),
        ) == BLST_ERROR::BLST_SUCCESS)
    }
}

impl Aggregatable<BLSSignature> for BLSSignature {
    type Error = anyhow::Error;

    fn aggregate(signatures: &[&BLSSignature]) -> anyhow::Result<BLSSignature> {
        let signatures = signatures
            .iter()
            .map(|signature| signature.to_blst_signature())
            .collect::<Result<Vec<_>, _>>()?;
        let aggregate_signature =
            BlstAggregateSignature::aggregate(&signatures.iter().collect::<Vec<_>>(), true)
                .map_err(|err| {
                    anyhow!("Failed to aggregate and validate BLST signatures {err:?}")
                })?;
        Ok(BLSSignature::try_from(aggregate_signature.to_signature())?)
    }
}

impl SupranationalAggregatable<BLSSignature> for BLSSignature {}

impl SupranationalVerifiable for BLSSignature {}
