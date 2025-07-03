use anyhow::anyhow;
use blst::min_pk::{AggregatePublicKey as BlstAggregatePublicKey, PublicKey as BlstPublicKey};
use ssz_types::FixedVector;

use crate::{
    errors::BLSError,
    public_key::PublicKey,
    traits::{Aggregatable, SupranationalAggregatable},
};

impl TryFrom<BlstPublicKey> for PublicKey {
    type Error = BLSError;

    fn try_from(value: BlstPublicKey) -> Result<Self, Self::Error> {
        Ok(PublicKey {
            inner: FixedVector::new(value.to_bytes().to_vec())
                .map_err(|_| BLSError::InvalidPublicKey)?,
        })
    }
}

impl PublicKey {
    pub fn to_blst_public_key(&self) -> Result<BlstPublicKey, BLSError> {
        BlstPublicKey::from_bytes(&self.inner).map_err(|err| BLSError::BlstError(err.into()))
    }
}

impl Aggregatable<PublicKey> for PublicKey {
    type Error = anyhow::Error;

    fn aggregate(public_keys: &[&PublicKey]) -> anyhow::Result<PublicKey> {
        let public_keys = public_keys
            .iter()
            .map(|public_key| public_key.to_blst_public_key())
            .collect::<Result<Vec<_>, _>>()?;
        let aggregate_public_key =
            BlstAggregatePublicKey::aggregate(&public_keys.iter().collect::<Vec<_>>(), true)
                .map_err(|err| anyhow!("Failed to aggregate and validate public keys {err:?}"))?;
        Ok(PublicKey::try_from(aggregate_public_key.to_public_key())?)
    }
}

impl SupranationalAggregatable<PublicKey> for PublicKey {}
