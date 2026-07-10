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
///
/// Finalization as seen from the canonical head, not irreversible economic finality.
///
/// Re-derived from the head each update, so it is reorg-mutable and can lower on a reorg.
/// Always an ancestor of the head, never monotone, never a safety guarantee.
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
