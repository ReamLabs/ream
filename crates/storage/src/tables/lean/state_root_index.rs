use std::sync::Arc;

use alloy_primitives::B256;
use redb::{Database, TableDefinition};

use crate::tables::{ssz_encoder::SSZEncoding, table::REDBTable};

pub struct LeanStateRootIndexTable {
    pub db: Arc<Database>,
}

/// Table definition for the State Root Index table
///
/// Key: state_root
/// Value: block_root
impl REDBTable for LeanStateRootIndexTable {
    const TABLE_DEFINITION: TableDefinition<'_, SSZEncoding<B256>, SSZEncoding<B256>> =
        TableDefinition::new("lean_state_root_index");

    type Key = B256;

    type KeyTableDefinition = SSZEncoding<B256>;

    type Value = B256;

    type ValueTableDefinition = SSZEncoding<B256>;

    fn database(&self) -> Arc<Database> {
        self.db.clone()
    }
}
