use std::sync::Arc;

use alloy_primitives::B256;
use redb::{Database, TableDefinition};

use crate::tables::{ssz_encoder::SSZEncoding, table::REDBTable};

pub struct OptimisticRootsTable {
    pub db: Arc<Database>,
}

/// Table definition for the Optimistic Roots table
///
/// Key: block_root
/// Value: bool
impl REDBTable for OptimisticRootsTable {
    const TABLE_DEFINITION: TableDefinition<'_, SSZEncoding<B256>, SSZEncoding<bool>> =
        TableDefinition::new("optimistic_roots");

    type Key = B256;

    type KeyTableDefinition = SSZEncoding<B256>;

    type Value = bool;

    type ValueTableDefinition = SSZEncoding<bool>;

    fn database(&self) -> Arc<Database> {
        self.db.clone()
    }
}
