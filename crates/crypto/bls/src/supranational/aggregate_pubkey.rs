use blst::min_pk::AggregatePublicKey as BlstAggregatePublicKey;

use crate::{
    aggregate_pubkey::AggregatePubKey,
    pubkey::PubKey,
    traits::{Aggregatable, SupranationalAggregatable},
};

impl Aggregatable for AggregatePubKey {
    type Error = anyhow::Error;

    fn aggregate(pubkeys: &[&PubKey]) -> anyhow::Result<Self> {
        let blst_pubkeys = pubkeys
            .iter()
            .map(|pk| pk.to_blst_pubkey())
            .collect::<Result<Vec<_>, _>>()?;
        let aggregate_pubkey =
            BlstAggregatePublicKey::aggregate(&blst_pubkeys.iter().collect::<Vec<_>>(), true)
                .map_err(|err| {
                    anyhow::anyhow!("Failed to aggregate and validate public keys {err:?}")
                })?;
        Ok(Self {
            inner: aggregate_pubkey.to_public_key().into(),
        })
    }
}

impl SupranationalAggregatable for AggregatePubKey {}
