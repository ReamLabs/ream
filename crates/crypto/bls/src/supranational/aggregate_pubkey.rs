use blst::min_pk::AggregatePublicKey;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use ssz_derive::{Decode, Encode};
use tree_hash_derive::TreeHash;

use crate::PubKey;

#[derive(Debug, PartialEq, Clone, Encode, Decode, TreeHash, Default)]
pub struct AggregatePubKey {
    pub inner: PubKey,
}

impl Serialize for AggregatePubKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.inner.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for AggregatePubKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let inner = PubKey::deserialize(deserializer)?;
        Ok(Self { inner })
    }
}

impl AggregatePubKey {
    pub fn to_pubkey(self) -> PubKey {
        self.inner
    }

    pub fn aggregate(pubkeys: &[&PubKey]) -> anyhow::Result<Self> {
        let blst_pubkeys = pubkeys
            .iter()
            .map(|pk| pk.to_blst_pubkey())
            .collect::<Result<Vec<_>, _>>()?;
        let aggregate_pubkey =
            AggregatePublicKey::aggregate(&blst_pubkeys.iter().collect::<Vec<_>>(), true).map_err(
                |err| anyhow::anyhow!("Failed to aggregate and validate public keys {err:?}"),
            )?;
        Ok(Self {
            inner: aggregate_pubkey.to_public_key().into(),
        })
    }
}
