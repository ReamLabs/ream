use std::sync::Arc;

use ream_consensus_misc::checkpoint::Checkpoint;
use redb::{Database, Durability, TableDefinition};

use crate::{
    errors::StoreError,
    tables::{field::Field, ssz_encoder::SSZEncoding},
};

/// Table definition for the Unrealized_Justified_Checkpoint table
///
/// Value: Checkpoint
pub(crate) const UNREALIZED_JUSTIFED_CHECKPOINT_FIELD: TableDefinition<
    &str,
    SSZEncoding<Checkpoint>,
> = TableDefinition::new("beacon_unrealized_justified_checkpoint");

const UNREALIZED_JUSTIFED_CHECKPOINT_KEY: &str = "unrealized_justified_checkpoint_key";

pub struct UnrealizedJustifiedCheckpointField {
    pub db: Arc<Database>,
}

impl Field for UnrealizedJustifiedCheckpointField {
    type Value = Checkpoint;

    fn get(&self) -> Result<Checkpoint, StoreError> {
        let read_txn = self.db.begin_read()?;

        let table = read_txn.open_table(UNREALIZED_JUSTIFED_CHECKPOINT_FIELD)?;
        let result = table
            .get(UNREALIZED_JUSTIFED_CHECKPOINT_KEY)?
            .ok_or(StoreError::FieldNotInitilized)?;
        Ok(result.value())
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
