use std::sync::Arc;

use alloy_primitives::B256;
use ream_consensus_lean::block::SignedBlockWithAttestation;
use redb::{Database, TableDefinition};

use crate::{
    cache::LeanCacheDB,
    tables::{ssz_encoder::SSZEncoding, table::REDBTable},
};

pub struct LeanPendingBlocksTable {
    pub db: Arc<Database>,
    pub cache: Option<Arc<LeanCacheDB>>,
}

/// Table definition for the Lean Block table
///
/// Key: block_root
/// Value: [SignedBlockWithAttestation]
impl REDBTable for LeanPendingBlocksTable {
    const TABLE_DEFINITION: TableDefinition<
        '_,
        SSZEncoding<B256>,
        SSZEncoding<SignedBlockWithAttestation>,
    > = TableDefinition::new("lean_pending_blocks");

    type Key = B256;

    type KeyTableDefinition = SSZEncoding<B256>;

    type Value = SignedBlockWithAttestation;

    type ValueTableDefinition = SSZEncoding<SignedBlockWithAttestation>;

    fn database(&self) -> Arc<Database> {
        self.db.clone()
    }
}

impl LeanPendingBlocksTable {
    pub fn contains_key(&self, key: B256) -> bool {
        matches!(self.get(key), Ok(Some(_)))
    }
}
