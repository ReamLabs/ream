use std::sync::Arc;

use ream_consensus::checkpoint::Checkpoint;
use redb::{Database, Durability, TableDefinition};

use super::{Field, SSZEncoding};
use crate::errors::StoreError;

/// Table definition for the Unrealized_Finalized_Checkpoint table
///
/// Value: Checkpoint
pub const UNREALIZED_FINALIZED_CHECKPOINT_FIELD: TableDefinition<&str, SSZEncoding<Checkpoint>> =
    TableDefinition::new("unrealized_finalized_checkpoint");

pub const UNREALIZED_FINALIZED_CHECKPOINT_FIELD_KEY: &str = "unrealized_finalized_checkpoint_key";

pub struct UnrealizedFinalizedCheckpointField {
    pub db: Arc<Database>,
}

impl Field for UnrealizedFinalizedCheckpointField {
    type Value = Checkpoint;

    fn get(&self) -> Result<Option<Self::Value>, StoreError> {
        let read_txn = self.db.begin_read()?;

        let table = read_txn.open_table(UNREALIZED_FINALIZED_CHECKPOINT_FIELD)?;
        let result = table.get(UNREALIZED_FINALIZED_CHECKPOINT_FIELD_KEY)?;
        Ok(result.map(|res| res.value()))
    }

    fn insert(&self, value: Self::Value) -> Result<(), StoreError> {
        let mut write_txn = self.db.begin_write()?;
        write_txn.set_durability(Durability::Immediate);
        let mut table = write_txn.open_table(UNREALIZED_FINALIZED_CHECKPOINT_FIELD)?;
        table.insert(UNREALIZED_FINALIZED_CHECKPOINT_FIELD_KEY, value)?;
        drop(table);
        write_txn.commit()?;
        Ok(())
    }
}
