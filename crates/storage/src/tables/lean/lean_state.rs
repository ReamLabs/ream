use std::sync::Arc;

use alloy_primitives::B256;
use ream_consensus_lean::state::LeanState;
use redb::{Database, Durability, TableDefinition};

use crate::{
    errors::StoreError,
    tables::{ssz_encoder::SSZEncoding, table::Table},
};

/// Table definition for the Lean State table
///
/// Key: block_root
/// Value: LeanState
pub(crate) const LEAN_STATE_TABLE: TableDefinition<SSZEncoding<B256>, SSZEncoding<LeanState>> =
    TableDefinition::new("lean_state");

pub struct LeanStateTable {
    pub db: Arc<Database>,
}

impl Table for LeanStateTable {
    type Key = B256;

    type Value = LeanState;

    fn get(&self, key: Self::Key) -> Result<Option<Self::Value>, StoreError> {
        let read_txn = self.db.begin_read()?;

        let table = read_txn.open_table(LEAN_STATE_TABLE)?;
        let result = table.get(key)?;
        Ok(result.map(|res| res.value()))
    }

    fn insert(&self, key: Self::Key, value: Self::Value) -> Result<(), StoreError> {
        let mut write_txn = self.db.begin_write()?;
        write_txn.set_durability(Durability::Immediate);
        let mut table = write_txn.open_table(LEAN_STATE_TABLE)?;
        table.insert(key, value)?;
        drop(table);
        write_txn.commit()?;
        Ok(())
    }
}
