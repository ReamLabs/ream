use std::sync::Arc;

use ream_consensus::checkpoint::Checkpoint;
use redb::{Database, Durability, TableDefinition};

use super::{Field, SSZEncoding};
use crate::errors::StoreError;

/// Table definition for the Unrealized_Justified_Checkpoint table
///
/// Value: Checkpoint
pub const UNREALIZED_JUSTIFED_CHECKPOINT_FIELD: TableDefinition<&str, SSZEncoding<Checkpoint>> =
    TableDefinition::new("unrealized_justified_checkpoint");

pub const UNREALIZED_JUSTIFED_CHECKPOINT_KEY: &str = "unrealized_justified_checkpoint_key";

pub struct UnrealizedJustifiedCheckpointField {
    pub db: Arc<Database>,
}

impl Field for UnrealizedJustifiedCheckpointField {
    type Value = Checkpoint;

    fn get(&self) -> Result<Option<Self::Value>, StoreError> {
        let read_txn = self.db.begin_read()?;

        let table = read_txn.open_table(UNREALIZED_JUSTIFED_CHECKPOINT_FIELD)?;
        let result = table.get(UNREALIZED_JUSTIFED_CHECKPOINT_KEY)?;
        Ok(result.map(|res| res.value()))
    }

    fn insert(&self, value: Self::Value) -> Result<(), StoreError> {
        let mut write_txn = self.db.begin_write()?;
        write_txn.set_durability(Durability::Immediate);
        let mut table = write_txn.open_table(UNREALIZED_JUSTIFED_CHECKPOINT_FIELD)?;
        table.insert(UNREALIZED_JUSTIFED_CHECKPOINT_KEY, value)?;
        drop(table);
        write_txn.commit()?;
        Ok(())
    }
}
