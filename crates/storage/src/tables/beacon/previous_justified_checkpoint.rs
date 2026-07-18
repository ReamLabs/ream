use std::sync::Arc;

use ream_consensus_misc::checkpoint::Checkpoint;
use redb::{Database, TableDefinition};

use crate::tables::{field::REDBField, ssz_encoder::SSZEncoding};

pub struct PreviousJustifiedCheckpointField {
    pub db: Arc<Database>,
}

impl REDBField for PreviousJustifiedCheckpointField {
    const FIELD_DEFINITION: TableDefinition<'_, &str, SSZEncoding<Checkpoint>> =
        TableDefinition::new("beacon_previous_justified_checkpoint");

    const KEY: &str = "previous_justified_checkpoint_key";

    type Value = Checkpoint;
    type ValueFieldDefinition = SSZEncoding<Checkpoint>;

    fn database(&self) -> Arc<Database> {
        self.db.clone()
    }
}
