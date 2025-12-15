use std::sync::Arc;

use alloy_primitives::B256;
use redb::{Database, TableDefinition};

use crate::tables::{ssz_encoder::SSZEncoding, table::REDBTable};

pub struct BlockTimelinessTable {
    pub db: Arc<Database>,
}

/// Table definition for the Block Timeliness table
///
/// Key: block_timeliness
/// Value: bool
impl REDBTable for BlockTimelinessTable {
    const TABLE_DEFINITION: TableDefinition<'_, SSZEncoding<B256>, SSZEncoding<bool>> =
        TableDefinition::new("beacon_block_timeliness");

    type Key = B256;

    type KeyTableDefinition = SSZEncoding<B256>;

    type Value = bool;

    type ValueTableDefinition = SSZEncoding<bool>;

    fn database(&self) -> Arc<Database> {
        self.db.clone()
    }
}
