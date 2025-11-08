use std::sync::Arc;

use ream_consensus_lean::checkpoint::Checkpoint;
use redb::{Database, Durability, ReadableDatabase, TableDefinition};

use crate::{
    errors::StoreError,
    tables::{field::Field, ssz_encoder::SSZEncoding},
};

/// Table definition for the Latest Finalized table
///
/// Value: [Checkpoint]
pub const LATEST_FINALIZED_FIELD: TableDefinition<&str, SSZEncoding<Checkpoint>> =
    TableDefinition::new("lean_latest_finalized");

const LATEST_FINALIZED_FIELD_KEY: &str = "latest_finalized_key";

pub struct LatestFinalizedField {
    pub db: Arc<Database>,
}

impl Field for LatestFinalizedField {
    type Value = Checkpoint;

    fn get(&self) -> Result<Checkpoint, StoreError> {
        let read_txn = self.db.begin_read()?;

        let table = read_txn.open_table(LATEST_FINALIZED_FIELD)?;
        let result = table
            .get(LATEST_FINALIZED_FIELD_KEY)?
            .ok_or(StoreError::FieldNotInitilized)?;
        Ok(result.value())
    }

    fn insert(&self, value: Self::Value) -> Result<(), StoreError> {
        let mut write_txn = self.db.begin_write()?;
        write_txn.set_durability(Durability::Immediate)?;
        let mut table = write_txn.open_table(LATEST_FINALIZED_FIELD)?;
        table.insert(LATEST_FINALIZED_FIELD_KEY, value)?;
        drop(table);
        write_txn.commit()?;
        Ok(())
    }

    fn remove(&self) -> Result<Option<Self::Value>, StoreError> {
        let write_txn = self.db.begin_write()?;
        let mut table = write_txn.open_table(LATEST_FINALIZED_FIELD)?;
        let value = table.remove(LATEST_FINALIZED_FIELD_KEY)?.map(|v| v.value());
        drop(table);
        write_txn.commit()?;
        Ok(value)
    }
}
