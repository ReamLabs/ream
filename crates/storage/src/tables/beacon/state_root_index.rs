use std::sync::Arc;

use alloy_primitives::B256;
use redb::{Database, Durability, TableDefinition};

use crate::{
    errors::StoreError,
    tables::{ssz_encoder::SSZEncoding, table::Table},
};

/// Table definition for the State Root Index table
///
/// Key: state_root
/// Value: block_root
pub(crate) const STATE_ROOT_INDEX_TABLE: TableDefinition<SSZEncoding<B256>, SSZEncoding<B256>> =
    TableDefinition::new("beacon_state_root_index");

pub struct StateRootIndexTable {
    pub db: Arc<Database>,
}

impl Table for StateRootIndexTable {
    type Key = B256;

    type Value = B256;

    fn get(&self, key: Self::Key) -> Result<Option<Self::Value>, StoreError> {
        let read_txn = self.db.begin_read()?;

        let table = read_txn.open_table(STATE_ROOT_INDEX_TABLE)?;
        let result = table.get(key)?;
        Ok(result.map(|res| res.value()))
    }

    fn insert(&self, key: Self::Key, value: Self::Value) -> Result<(), StoreError> {
        let mut write_txn = self.db.begin_write()?;
        write_txn.set_durability(Durability::Immediate);
        let mut table = write_txn.open_table(STATE_ROOT_INDEX_TABLE)?;
        table.insert(key, value)?;
        drop(table);
        write_txn.commit()?;
        Ok(())
    }
}
