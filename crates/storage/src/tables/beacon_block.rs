use std::sync::Arc;

use alloy_primitives::B256;
use ream_consensus::deneb::beacon_block::BeaconBlock;
use redb::{Database, Durability, TableDefinition};

use super::{SSZEncoding, Table};
use crate::errors::StoreError;

/// Table definition for the Beacon Block table
///
/// Key: blocks
/// Value: BeaconBlock
pub const BEACON_BLOCK_TABLE: TableDefinition<SSZEncoding<B256>, SSZEncoding<BeaconBlock>> =
    TableDefinition::new("beacon_block");

pub struct BeaconBlockTable {
    pub db: Arc<Database>,
}

impl Table for BeaconBlockTable {
    type Key = B256;

    type Value = BeaconBlock;

    fn get(&self, key: Self::Key) -> Result<Option<Self::Value>, StoreError> {
        let read_txn = self.db.begin_read()?;

        let table = read_txn.open_table(BEACON_BLOCK_TABLE)?;
        let result = table.get(key)?;
        Ok(result.map(|res| res.value()))
    }

    fn insert(&self, key: Self::Key, value: Self::Value) -> Result<(), StoreError> {
        let mut write_txn = self.db.begin_write()?;
        write_txn.set_durability(Durability::Immediate);
        let mut table = write_txn.open_table(BEACON_BLOCK_TABLE)?;
        table.insert(key, value)?;
        drop(table);
        write_txn.commit()?;
        Ok(())
    }
}
