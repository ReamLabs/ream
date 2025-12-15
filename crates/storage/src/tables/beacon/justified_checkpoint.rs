use std::sync::Arc;

use ream_consensus_misc::checkpoint::Checkpoint;
use redb::{Database, TableDefinition};

use crate::tables::{field::REDBField, ssz_encoder::SSZEncoding};

pub struct JustifiedCheckpointField {
    pub db: Arc<Database>,
}

/// Table definition for the Justified_Checkpoint table
///
/// Value: Checkpoint
impl REDBField for JustifiedCheckpointField {
    const FIELD_DEFINITION: TableDefinition<'_, &str, SSZEncoding<Checkpoint>> =
        TableDefinition::new("beacon_justified_checkpoint");

    const KEY: &str = "justified_checkpoint_key";

    type Value = Checkpoint;

    type ValueFieldDefinition = SSZEncoding<Checkpoint>;

    fn database(&self) -> Arc<Database> {
        self.db.clone()
    }
}
