use std::sync::Arc;

use alloy_primitives::B256;
use ream_consensus_lean::state::LeanState;
use redb::{Database, TableDefinition};

use crate::tables::{ssz_encoder::SSZEncoding, table::REDBTable};

pub struct LeanStateTable {
    pub db: Arc<Database>,
}

/// Table definition for the Lean State table
///
/// Key: block_root
/// Value: [LeanState]
impl REDBTable for LeanStateTable {
    const TABLE_DEFINITION: TableDefinition<'_, SSZEncoding<B256>, SSZEncoding<LeanState>> =
        TableDefinition::new("lean_state");

    type Key = B256;

    type KeyTableDefinition = SSZEncoding<B256>;

    type Value = LeanState;

    type ValueTableDefinition = SSZEncoding<LeanState>;

    fn database(&self) -> Arc<Database> {
        self.db.clone()
    }
}
