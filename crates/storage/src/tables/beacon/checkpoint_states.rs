use std::sync::Arc;

use ream_consensus_beacon::electra::beacon_state::BeaconState;
use ream_consensus_misc::checkpoint::Checkpoint;
use redb::{Database, TableDefinition};

use crate::tables::{ssz_encoder::SSZEncoding, table::REDBTable};

pub struct CheckpointStatesTable {
    pub db: Arc<Database>,
}

/// Table definition for the Checkpoint States table
///
/// Key: checkpoint_states
/// Value: BeaconState
impl REDBTable for CheckpointStatesTable {
    const TABLE_DEFINITION: TableDefinition<'_, SSZEncoding<Checkpoint>, SSZEncoding<BeaconState>> =
        TableDefinition::new("beacon_checkpoint_states");

    type Key = Checkpoint;

    type KeyTableDefinition = SSZEncoding<Checkpoint>;

    type Value = BeaconState;

    type ValueTableDefinition = SSZEncoding<BeaconState>;

    fn database(&self) -> Arc<Database> {
        self.db.clone()
    }
}
