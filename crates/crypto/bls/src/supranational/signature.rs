use blst::{min_pk::Signature as BlstSignature, BLST_ERROR};

use crate::{
    constants::DST,
    errors::BLSError,
    pubkey::PubKey,
    signature::BLSSignature,
    traits::{SupranationalVerifiable, Verifiable},
};

impl BLSSignature {
    pub fn to_blst_signature(&self) -> Result<BlstSignature, BLSError> {
        BlstSignature::from_bytes(&self.inner).map_err(|e| BLSError::BlstError(e.into()))
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

impl SupranationalVerifiable for BLSSignature {}
