use std::sync::Arc;

use alloy_primitives::B256;
use ream_consensus::checkpoint::Checkpoint;
use redb::{Database, Durability, TableDefinition};

use super::{SSZEncoding, Table};
use crate::errors::StoreError;

/// Table definition for the Unrealized Justifications table
///
/// Key: unrealized_justifications
/// Value: Checkpoint
pub const UNREALIZED_JUSTIFICATIONS_TABLE: TableDefinition<
    SSZEncoding<B256>,
    SSZEncoding<Checkpoint>,
> = TableDefinition::new("unrealized_justifications");

pub struct UnrealizedJustificationsTable {
    pub db: Arc<Database>,
}

impl Table for UnrealizedJustificationsTable {
    type Key = B256;

    type Value = Checkpoint;

    fn get(&self, key: Self::Key) -> Result<Option<Self::Value>, StoreError> {
        let read_txn = self.db.begin_read()?;

        let table = read_txn.open_table(UNREALIZED_JUSTIFICATIONS_TABLE)?;
        let result = table.get(key)?;
        Ok(result.map(|res| res.value()))
    }

    fn insert(&self, key: Self::Key, value: Self::Value) -> Result<(), StoreError> {
        let mut write_txn = self.db.begin_write()?;
        write_txn.set_durability(Durability::Immediate);
        let mut table = write_txn.open_table(UNREALIZED_JUSTIFICATIONS_TABLE)?;
        table.insert(key, value)?;
        drop(table);
        write_txn.commit()?;
        Ok(())
    }
}
