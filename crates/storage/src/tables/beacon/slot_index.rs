use std::sync::Arc;

use alloy_primitives::B256;
use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};

use crate::{
    errors::StoreError,
    tables::{ssz_encoder::SSZEncoding, table::REDBTable},
};

pub struct BeaconSlotIndexTable {
    pub db: Arc<Database>,
}

/// Table definition for the Slot Index table
///
/// Key: slot number
/// Value: block_root
impl REDBTable for BeaconSlotIndexTable {
    const TABLE_DEFINITION: TableDefinition<'_, u64, SSZEncoding<B256>> =
        TableDefinition::new("beacon_slot_index");

    type Key = u64;

    type KeyTableDefinition = u64;

    type Value = B256;

    type ValueTableDefinition = SSZEncoding<B256>;

    fn database(&self) -> Arc<Database> {
        self.db.clone()
    }
}

impl BeaconSlotIndexTable {
    pub fn get_oldest_slot(&self) -> Result<Option<u64>, StoreError> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(Self::TABLE_DEFINITION)?;
        Ok(table.first()?.map(|result| result.0.value()))
    }

    pub fn get_oldest_root(&self) -> Result<Option<B256>, StoreError> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(Self::TABLE_DEFINITION)?;
        Ok(table.first()?.map(|result| result.1.value()))
    }

    pub fn get_highest_slot(&self) -> Result<Option<u64>, StoreError> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(Self::TABLE_DEFINITION)?;
        Ok(table.last()?.map(|result| result.0.value()))
    }

    pub fn get_highest_root(&self) -> Result<Option<B256>, StoreError> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(Self::TABLE_DEFINITION)?;
        Ok(table.last()?.map(|result| result.1.value()))
    }
}
