use anyhow::anyhow;
use blst::min_pk::{AggregatePublicKey as BlstAggregatePublicKey, PublicKey as BlstPublicKey};
use ssz_types::FixedVector;

use crate::{
    errors::BLSError,
    pubkey::PubKey,
    traits::{Aggregatable, SupranationalAggregatable},
};

impl TryFrom<BlstPublicKey> for PubKey {
    type Error = BLSError;

    fn try_from(value: BlstPublicKey) -> Result<Self, Self::Error> {
        Ok(PubKey {
            inner: FixedVector::new(value.to_bytes().to_vec())
                .map_err(|_| BLSError::InvalidPublicKey)?,
        })
    }
}

impl PubKey {
    pub fn to_blst_pubkey(&self) -> Result<BlstPublicKey, BLSError> {
        BlstPublicKey::from_bytes(&self.inner).map_err(|err| BLSError::BlstError(err.into()))
    }
}

impl Aggregatable<PubKey> for PubKey {
    type Error = anyhow::Error;

    fn aggregate(public_keys: &[&PubKey]) -> anyhow::Result<PubKey> {
        let public_keys = public_keys
            .iter()
            .map(|public_key| public_key.to_blst_pubkey())
            .collect::<Result<Vec<_>, _>>()?;
        let aggregate_public_key =
            BlstAggregatePublicKey::aggregate(&public_keys.iter().collect::<Vec<_>>(), true)
                .map_err(|err| anyhow!("Failed to aggregate and validate public keys {err:?}"))?;
        Ok(PubKey::try_from(aggregate_public_key.to_public_key())?)
    }
}

impl SupranationalAggregatable<PubKey> for PubKey {}
