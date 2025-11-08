use std::sync::Arc;

use ream_consensus_lean::checkpoint::Checkpoint;
use redb::{Database, TableDefinition};

use crate::tables::{field::REDBField, ssz_encoder::SSZEncoding};

pub struct LatestJustifiedField {
    pub db: Arc<Database>,
}

/// Table definition for the Latest Justified table
///
/// Value: [Checkpoint]
///
/// NOTE: This table enables O(1) access to the latest justified checkpoint, deviates from
/// the original spec which derives it from state dictionary each time it is needed.
impl REDBField for LatestJustifiedField {
    const FIELD_DEFINITION: TableDefinition<'_, &str, SSZEncoding<Checkpoint>> =
        TableDefinition::new("lean_latest_justified");

    const KEY: &str = "latest_justified_key";

    type Value = Checkpoint;

    type ValueFieldDefinition = SSZEncoding<Checkpoint>;

    fn database(&self) -> Arc<Database> {
        self.db.clone()
    }
}
