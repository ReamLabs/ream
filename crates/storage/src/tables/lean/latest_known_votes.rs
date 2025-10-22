use std::{collections::HashMap, sync::Arc};

use ream_consensus_lean::vote::SignedAttestation;
use redb::{Database, Durability, ReadableDatabase, ReadableTable, TableDefinition};

use crate::{
    errors::StoreError,
    tables::{ssz_encoder::SSZEncoding, table::Table},
};

/// Table definition for the Latest Known Votes table
///
/// Key: u64 (validator index)
/// Value: [SignedVote]
pub(crate) const LATEST_KNOWN_VOTES_TABLE: TableDefinition<u64, SSZEncoding<SignedAttestation>> =
    TableDefinition::new("latest_known_votes");

pub struct LatestKnownVotesTable {
    pub db: Arc<Database>,
}

impl Table for LatestKnownVotesTable {
    type Key = u64;

    type Value = SignedAttestation;

    fn get(&self, key: Self::Key) -> Result<Option<Self::Value>, StoreError> {
        let read_txn = self.db.begin_read()?;

        let table = read_txn.open_table(LATEST_KNOWN_VOTES_TABLE)?;
        let result = table.get(key)?;
        Ok(result.map(|res| res.value()))
    }

    fn insert(&self, key: Self::Key, value: Self::Value) -> Result<(), StoreError> {
        let mut write_txn = self.db.begin_write()?;
        write_txn.set_durability(Durability::Immediate)?;
        let mut table = write_txn.open_table(LATEST_KNOWN_VOTES_TABLE)?;
        table.insert(key, value)?;
        drop(table);
        write_txn.commit()?;
        Ok(())
    }
}

impl LatestKnownVotesTable {
    /// Insert multiple votes with validator id in a single transaction.
    pub fn batch_insert(
        &self,
        values: impl IntoIterator<Item = (u64, SignedAttestation)>,
    ) -> Result<(), StoreError> {
        let mut write_txn = self.db.begin_write()?;
        write_txn.set_durability(Durability::Immediate)?;

        let mut table = write_txn.open_table(LATEST_KNOWN_VOTES_TABLE)?;

        for (key, value) in values {
            table.insert(key, value)?;
        }

        drop(table);
        write_txn.commit()?;

        Ok(())
    }

    /// Get all votes.
    pub fn get_all_votes(&self) -> Result<HashMap<u64, SignedAttestation>, StoreError> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(LATEST_KNOWN_VOTES_TABLE)?;

        table
            .iter()?
            .map(|entry| {
                let (k, v) = entry?;
                Ok((k.value(), v.value()))
            })
            .collect()
    }
}
