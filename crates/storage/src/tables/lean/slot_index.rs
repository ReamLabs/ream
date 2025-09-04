use std::sync::Arc;

use alloy_primitives::B256;
use redb::{Database, Durability, ReadableTable, TableDefinition};

use crate::{
    errors::StoreError,
    tables::{ssz_encoder::SSZEncoding, table::Table},
};

/// Table definition for the Slot Index table
///
/// Key: slot number
/// Value: block_root
pub(crate) const LEAN_SLOT_INDEX_TABLE: TableDefinition<u64, SSZEncoding<B256>> =
    TableDefinition::new("lean_slot_index");

pub struct SlotIndexTable {
    pub db: Arc<Database>,
}

impl Table for SlotIndexTable {
    type Key = u64;

    type Value = B256;

    fn get(&self, key: Self::Key) -> Result<Option<Self::Value>, StoreError> {
        let read_txn = self.db.begin_read()?;

        let table = read_txn.open_table(LEAN_SLOT_INDEX_TABLE)?;
        let result = table.get(key)?;
        Ok(result.map(|res| res.value()))
    }

    fn insert(&self, key: Self::Key, value: Self::Value) -> Result<(), StoreError> {
        let mut write_txn = self.db.begin_write()?;
        write_txn.set_durability(Durability::Immediate);
        let mut table = write_txn.open_table(LEAN_SLOT_INDEX_TABLE)?;
        table.insert(key, value)?;
        drop(table);
        write_txn.commit()?;
        Ok(())
    }
}

impl SlotIndexTable {
    pub fn get_oldest_slot(&self) -> Result<Option<u64>, StoreError> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(LEAN_SLOT_INDEX_TABLE)?;
        Ok(table.first()?.map(|result| result.0.value()))
    }

    pub fn get_oldest_root(&self) -> Result<Option<B256>, StoreError> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(LEAN_SLOT_INDEX_TABLE)?;
        Ok(table.first()?.map(|result| result.1.value()))
    }

    pub fn get_highest_slot(&self) -> Result<Option<u64>, StoreError> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(LEAN_SLOT_INDEX_TABLE)?;
        Ok(table.last()?.map(|result| result.0.value()))
    }

    pub fn get_highest_root(&self) -> Result<Option<B256>, StoreError> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(LEAN_SLOT_INDEX_TABLE)?;
        Ok(table.last()?.map(|result| result.1.value()))
    }
}
