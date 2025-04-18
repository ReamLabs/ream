use std::sync::Arc;

use ream_consensus::checkpoint::Checkpoint;
use redb::{Database, Durability, TableDefinition};

use super::{Field, SSZEncoding};
use crate::errors::StoreError;

/// Table definition for the Finalized_Checkpoint table
///
/// Value: Checkpoint
pub const FINALIZED_CHECKPOINT_FIELD: TableDefinition<&str, SSZEncoding<Checkpoint>> =
    TableDefinition::new("finalized_checkpoint");

pub const FINALIZED_CHECKPOINT_FIELD_KEY: &str = "finalized_checkpoint_key";

pub struct FinalizedCheckpointField {
    pub db: Arc<Database>,
}

impl Field for FinalizedCheckpointField {
    type Value = Checkpoint;

    fn get(&self) -> Result<Option<Self::Value>, StoreError> {
        let read_txn = self.db.begin_read()?;

        let table = read_txn.open_table(FINALIZED_CHECKPOINT_FIELD)?;
        let result = table.get(FINALIZED_CHECKPOINT_FIELD_KEY)?;
        Ok(result.map(|res| res.value()))
    }

    fn insert(&self, value: Self::Value) -> Result<(), StoreError> {
        let mut write_txn = self.db.begin_write()?;
        write_txn.set_durability(Durability::Immediate);
        let mut table = write_txn.open_table(FINALIZED_CHECKPOINT_FIELD)?;
        table.insert(FINALIZED_CHECKPOINT_FIELD_KEY, value)?;
        drop(table);
        write_txn.commit()?;
        Ok(())
    }
}
