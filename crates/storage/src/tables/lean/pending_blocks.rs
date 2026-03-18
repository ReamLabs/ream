use std::sync::Arc;

use alloy_primitives::B256;
#[cfg(feature = "devnet4")]
use ream_consensus_lean::block::SignedBlock;
#[cfg(feature = "devnet3")]
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
    #[cfg(feature = "devnet3")]
    const TABLE_DEFINITION: TableDefinition<
        '_,
        SSZEncoding<B256>,
        SSZEncoding<SignedBlockWithAttestation>,
    > = TableDefinition::new("lean_pending_blocks");
    #[cfg(feature = "devnet4")]
    const TABLE_DEFINITION: TableDefinition<'_, SSZEncoding<B256>, SSZEncoding<SignedBlock>> =
        TableDefinition::new("lean_pending_blocks");

    type Key = B256;

    type KeyTableDefinition = SSZEncoding<B256>;
    #[cfg(feature = "devnet3")]
    type Value = SignedBlockWithAttestation;
    #[cfg(feature = "devnet3")]
    type ValueTableDefinition = SSZEncoding<SignedBlockWithAttestation>;
    #[cfg(feature = "devnet4")]
    type Value = SignedBlock;
    #[cfg(feature = "devnet4")]
    type ValueTableDefinition = SSZEncoding<SignedBlock>;

    fn database(&self) -> Arc<Database> {
        self.db.clone()
    }
}

impl LeanPendingBlocksTable {
    pub fn contains_key(&self, key: B256) -> bool {
        matches!(self.get(key), Ok(Some(_)))
    }
}
