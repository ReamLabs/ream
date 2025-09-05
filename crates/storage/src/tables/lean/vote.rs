use std::sync::Arc;

use ream_consensus_lean::vote::Vote;
use redb::{Database, Durability, TableDefinition};

use crate::{
    errors::StoreError,
    tables::{ssz_encoder::SSZEncoding, table::Table},
};

/// Table definition for the Lean Vote table
///
/// Key: slot
/// Value: `Vote`
pub(crate) const LEAN_VOTE_TABLE: TableDefinition<u64, SSZEncoding<Vote>> =
    TableDefinition::new("lean_vote");

pub struct LeanVoteTable {
    pub db: Arc<Database>,
}

impl Table for LeanVoteTable {
    type Key = u64;

    type Value = Vote;

    fn get(&self, key: Self::Key) -> Result<Option<Self::Value>, StoreError> {
        let read_txn = self.db.begin_read()?;

        let table = read_txn.open_table(LEAN_VOTE_TABLE)?;
        let result = table.get(key)?;
        Ok(result.map(|res| res.value()))
    }

    fn insert(&self, key: Self::Key, value: Self::Value) -> Result<(), StoreError> {
        let mut write_txn = self.db.begin_write()?;
        write_txn.set_durability(Durability::Immediate);

        let mut table = write_txn.open_table(LEAN_VOTE_TABLE)?;
        table.insert(key, value)?;

        drop(table);
        write_txn.commit()?;
        Ok(())
    }
}
