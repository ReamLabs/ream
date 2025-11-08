use std::sync::Arc;

use alloy_primitives::B256;
use ream_consensus_misc::checkpoint::Checkpoint;
use redb::{Database, TableDefinition};

use crate::tables::{ssz_encoder::SSZEncoding, table::REDBTable};

pub struct UnrealizedJustificationsTable {
    pub db: Arc<Database>,
}

/// Table definition for the Unrealized Justifications table
///
/// Key: unrealized_justifications
/// Value: Checkpoint
impl REDBTable for UnrealizedJustificationsTable {
    const TABLE_DEFINITION: TableDefinition<'_, SSZEncoding<B256>, SSZEncoding<Checkpoint>> =
        TableDefinition::new("beacon_unrealized_justifications");

    type Key = B256;

    type KeyTableDefinition = SSZEncoding<B256>;

    type Value = Checkpoint;

    type ValueTableDefinition = SSZEncoding<Checkpoint>;

    fn database(&self) -> Arc<Database> {
        self.db.clone()
    }
}
