use std::sync::Arc;

use alloy_primitives::B256;
use ream_consensus_lean::block::SignedBlock;
use redb::{Database, Durability, ReadableDatabase, ReadableTableMetadata, TableDefinition};

use crate::{
    cache::LeanCacheDB,
    errors::StoreError,
    tables::{ssz_encoder::SSZEncoding, table::REDBTable},
};

pub struct LeanPendingBlocksTable {
    pub db: Arc<Database>,
    pub cache: Option<Arc<LeanCacheDB>>,
}

/// Table definition for the Lean Block table
///
/// Key: block_root
/// Value: [SignedBlock]
impl REDBTable for LeanPendingBlocksTable {
    const TABLE_DEFINITION: TableDefinition<'_, SSZEncoding<B256>, SSZEncoding<SignedBlock>> =
        TableDefinition::new("lean_pending_blocks");

    type Key = B256;

    type KeyTableDefinition = SSZEncoding<B256>;

    type Value = SignedBlock;

    type ValueTableDefinition = SSZEncoding<SignedBlock>;

    fn database(&self) -> Arc<Database> {
        self.db.clone()
    }
}

impl LeanPendingBlocksTable {
    pub fn iter(
        &self,
    ) -> Result<
        impl Iterator<Item = Result<(B256, <Self as REDBTable>::Value), StoreError>>,
        StoreError,
    > {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(Self::TABLE_DEFINITION)?;
        Ok(table
            .range::<<SSZEncoding<B256> as redb::Value>::SelfType<'_>>(..)?
            .map(|result| {
                result
                    .map(|(key, value)| (key.value(), value.value()))
                    .map_err(StoreError::from)
            }))
    }

    pub fn retain<F>(&self, mut f: F) -> Result<(), crate::errors::StoreError>
    where
        F: FnMut(&B256, &<Self as REDBTable>::Value) -> bool,
    {
        let mut write_txn = self.db.begin_write()?;
        write_txn.set_durability(Durability::Immediate)?;
        {
            let mut table = write_txn.open_table(Self::TABLE_DEFINITION)?;
            table.retain(|key, value| f(&key, &value))?;
        }
        write_txn.commit()?;
        Ok(())
    }

    pub fn contains_key(&self, key: B256) -> bool {
        matches!(self.get(key), Ok(Some(_)))
    }

    pub fn len(&self) -> usize {
        let Ok(read_txn) = self.db.begin_read() else {
            return 0;
        };
        let Ok(table) = read_txn.open_table(Self::TABLE_DEFINITION) else {
            return 0;
        };
        table.len().unwrap_or(0) as usize
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
