use std::sync::Arc;

use ream_consensus_lean::checkpoint::Checkpoint;
use redb::{Database, Durability, ReadableDatabase, TableDefinition};

use crate::{
    errors::StoreError,
    tables::{field::Field, ssz_encoder::SSZEncoding},
};

/// Table definition for the Latest Justified table
///
/// Value: [Checkpoint]
///
/// NOTE: This table enables O(1) access to the latest justified checkpoint, deviates from
/// the original spec which derives it from state dictionary each time it is needed.
pub const LATEST_JUSTIFIED_FIELD: TableDefinition<&str, SSZEncoding<Checkpoint>> =
    TableDefinition::new("lean_latest_justified");

const LATEST_JUSTIFIED_FIELD_KEY: &str = "latest_justified_key";

pub struct LatestJustifiedField {
    pub db: Arc<Database>,
}

impl Field for LatestJustifiedField {
    type Value = Checkpoint;

    fn get(&self) -> Result<Checkpoint, StoreError> {
        let read_txn = self.db.begin_read()?;

        let table = read_txn.open_table(LATEST_JUSTIFIED_FIELD)?;
        let result = table
            .get(LATEST_JUSTIFIED_FIELD_KEY)?
            .ok_or(StoreError::FieldNotInitilized)?;
        Ok(result.value())
    }

    fn insert(&self, value: Self::Value) -> Result<(), StoreError> {
        let mut write_txn = self.db.begin_write()?;
        write_txn.set_durability(Durability::Immediate)?;
        let mut table = write_txn.open_table(LATEST_JUSTIFIED_FIELD)?;
        table.insert(LATEST_JUSTIFIED_FIELD_KEY, value)?;
        drop(table);
        write_txn.commit()?;
        Ok(())
    }

    fn remove(&self) -> Result<Option<Self::Value>, StoreError> {
        let write_txn = self.db.begin_write()?;
        let mut table = write_txn.open_table(LATEST_JUSTIFIED_FIELD)?;
        let value = table.remove(LATEST_JUSTIFIED_FIELD_KEY)?.map(|v| v.value());
        drop(table);
        write_txn.commit()?;
        Ok(value)
    }
}
