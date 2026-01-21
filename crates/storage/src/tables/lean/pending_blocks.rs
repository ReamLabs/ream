use std::sync::Arc;

use alloy_primitives::B256;
use ream_consensus_lean::block::SignedBlockWithAttestation;
use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};

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

    /// Returns all cached blocks as (root, block) pairs.
    pub fn get_all(
        &self,
    ) -> Result<Vec<(B256, SignedBlockWithAttestation)>, crate::errors::StoreError> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(Self::TABLE_DEFINITION)?;
        table
            .iter()?
            .map(|entry| {
                let (k, v) = entry?;
                Ok((k.value(), v.value()))
            })
            .collect()
    }
}
