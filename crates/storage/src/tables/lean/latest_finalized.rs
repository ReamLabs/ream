use std::sync::Arc;

use ream_consensus_lean::checkpoint::Checkpoint;
use redb::{Database, TableDefinition};

use crate::tables::{field::REDBField, ssz_encoder::SSZEncoding};

pub struct LatestFinalizedField {
    pub db: Arc<Database>,
}

/// Table definition for the Latest Finalized table
///
/// Value: [Checkpoint]
impl REDBField for LatestFinalizedField {
    const FIELD_DEFINITION: TableDefinition<'_, &str, SSZEncoding<Checkpoint>> =
        TableDefinition::new("lean_latest_finalized");

    const KEY: &str = "latest_finalized_key";

    type Value = Checkpoint;

    type ValueFieldDefinition = SSZEncoding<Checkpoint>;

    fn database(&self) -> Arc<Database> {
        self.db.clone()
    }
}
