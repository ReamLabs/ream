use std::{collections::HashMap, sync::Arc};

use ream_consensus_lean::vote::SignedVote;
use redb::{Database, Durability, ReadableTable, TableDefinition};

use crate::{errors::StoreError, tables::ssz_encoder::SSZEncoding};

/// Table definition for the Latest Known Votes table
///
/// Key: u64 (validator index)
/// Value: [SignedVote]
pub(crate) const LATEST_KNOWN_VOTES_TABLE: TableDefinition<u64, SSZEncoding<SignedVote>> =
    TableDefinition::new("latest_known_votes");

pub struct LatestKnownVotesTable {
    pub db: Arc<Database>,
}

impl LatestKnownVotesTable {
    /// Insert multiple votes with validator id in a single transaction.
    pub fn batch_insert(
        &self,
        values: impl IntoIterator<Item = (u64, SignedVote)>,
    ) -> Result<(), StoreError> {
        let mut write_txn = self.db.begin_write()?;
        write_txn.set_durability(Durability::Immediate);

        let mut table = write_txn.open_table(LATEST_KNOWN_VOTES_TABLE)?;

        for (key, value) in values {
            table.insert(key, value)?;
        }

        drop(table);
        write_txn.commit()?;

        Ok(())
    }

    /// Check if a given vote exists in the append-only array.
    pub fn contains(&self, value: &SignedVote) -> Result<bool, StoreError> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(LATEST_KNOWN_VOTES_TABLE)?;

        for entry in table.iter()? {
            let (_, v) = entry?;
            if &v.value() == value {
                return Ok(true);
            }
        }

        Ok(false)
    }

    /// Get all votes.
    pub fn get_all_votes(&self) -> Result<HashMap<u64, SignedVote>, StoreError> {
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
