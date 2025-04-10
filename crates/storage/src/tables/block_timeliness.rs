use std::sync::Arc;

use alloy_primitives::B256;
use redb::{Database, Durability, TableDefinition};

use super::{SSZEncoding, Table};
use crate::errors::StoreError;

/// Table definition for the Block Timeliness table
///
/// Key: block_timeliness
/// Value: bool
pub const BLOCK_TIMELINESS_TABLE: TableDefinition<SSZEncoding<B256>, SSZEncoding<bool>> =
    TableDefinition::new("block_timeliness");

pub struct BlockTimelinessTable {
    pub db: Arc<Database>,
}

impl Table for BlockTimelinessTable {
    type Key = B256;

    type Value = bool;

    fn get(&self, key: Self::Key) -> Result<Option<Self::Value>, StoreError> {
        let read_txn = self.db.begin_read()?;

        let table = read_txn.open_table(BLOCK_TIMELINESS_TABLE)?;
        let result = table.get(key)?;
        Ok(result.map(|res| res.value()))
    }

    fn insert(&self, key: Self::Key, value: Self::Value) -> Result<(), StoreError> {
        let mut write_txn = self.db.begin_write()?;
        write_txn.set_durability(Durability::Immediate);
        let mut table = write_txn.open_table(BLOCK_TIMELINESS_TABLE)?;
        table.insert(key, value)?;
        drop(table);
        write_txn.commit()?;
        Ok(())
    }
}
