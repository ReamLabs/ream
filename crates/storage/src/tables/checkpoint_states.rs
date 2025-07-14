use std::sync::Arc;

use ream_consensus_beacon::electra::beacon_state::BeaconState;
use ream_consensus_misc::checkpoint::Checkpoint;
use redb::{Database, Durability, TableDefinition};

use super::{SSZEncoding, Table};
use crate::errors::StoreError;

/// Table definition for the Checkpoint States table
///
/// Key: checkpoint_states
/// Value: BeaconState
pub const CHECKPOINT_STATES_TABLE: TableDefinition<
    SSZEncoding<Checkpoint>,
    SSZEncoding<BeaconState>,
> = TableDefinition::new("checkpoint_states");

pub struct CheckpointStatesTable {
    pub db: Arc<Database>,
}

impl Table for CheckpointStatesTable {
    type Key = Checkpoint;

    type Value = BeaconState;

    fn get(&self, key: Self::Key) -> Result<Option<Self::Value>, StoreError> {
        let read_txn = self.db.begin_read()?;

        let table = read_txn.open_table(CHECKPOINT_STATES_TABLE)?;
        let result = table.get(key)?;
        Ok(result.map(|res| res.value()))
    }

    fn insert(&self, key: Self::Key, value: Self::Value) -> Result<(), StoreError> {
        let mut write_txn = self.db.begin_write()?;
        write_txn.set_durability(Durability::Immediate);
        let mut table = write_txn.open_table(CHECKPOINT_STATES_TABLE)?;
        table.insert(key, value)?;
        drop(table);
        write_txn.commit()?;
        Ok(())
    }
}
