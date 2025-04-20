use std::sync::Arc;

use ream_consensus::checkpoint::Checkpoint;
use redb::{Database, Durability, TableDefinition};

use super::{Field, SSZEncoding};
use crate::errors::StoreError;

/// Table definition for the Justified_Checkpoint table
///
/// Value: Checkpoint
pub const JUSTIFIED_CHECKPOINT_FIELD: TableDefinition<&str, SSZEncoding<Checkpoint>> =
    TableDefinition::new("justified_checkpoint");

pub const JUSTIFIED_CHECKPOINT_KEY: &str = "justified_checkpoint_key";

pub struct JustifiedCheckpointField {
    pub db: Arc<Database>,
}

impl Field for JustifiedCheckpointField {
    type Value = Checkpoint;

    fn get(&self) -> Result<Checkpoint, StoreError> {
        let read_txn = self.db.begin_read()?;

        let table = read_txn.open_table(JUSTIFIED_CHECKPOINT_FIELD)?;
        let result = table
            .get(JUSTIFIED_CHECKPOINT_KEY)?
            .ok_or(StoreError::FieldNotInitilized)?;
        Ok(result.value())
    }

    fn insert(&self, value: Self::Value) -> Result<(), StoreError> {
        let mut write_txn = self.db.begin_write()?;
        write_txn.set_durability(Durability::Immediate);
        let mut table = write_txn.open_table(JUSTIFIED_CHECKPOINT_FIELD)?;
        table.insert(JUSTIFIED_CHECKPOINT_KEY, value)?;
        drop(table);
        write_txn.commit()?;
        Ok(())
    }
}
