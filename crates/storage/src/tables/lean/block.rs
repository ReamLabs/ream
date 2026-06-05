use std::{collections::HashMap, sync::Arc};

use alloy_primitives::B256;
use ream_consensus_lean::block::SignedBlock;
use redb::{Database, Durability, ReadableDatabase, ReadableTable, TableDefinition};
use tree_hash::TreeHash;

use super::{
    children_index::{ChildIndexEntry, LeanChildrenIndexTable},
    slot_index::LeanSlotIndexTable,
    state_root_index::LeanStateRootIndexTable,
};
use crate::{
    cache::LeanCacheDB,
    errors::StoreError,
    tables::{ssz_encoder::SSZEncoding, table::REDBTable},
};

pub struct LeanBlockTable {
    pub db: Arc<Database>,
    pub cache: Option<Arc<LeanCacheDB>>,
}

/// Table definition for the Lean Block table
///
/// Key: block_root
/// Value: [SignedBlock]
impl REDBTable for LeanBlockTable {
    const TABLE_DEFINITION: TableDefinition<'_, SSZEncoding<B256>, SSZEncoding<SignedBlock>> =
        TableDefinition::new("lean_block");

    type Key = B256;

    type KeyTableDefinition = SSZEncoding<B256>;

    type Value = SignedBlock;

    type ValueTableDefinition = SSZEncoding<SignedBlock>;

    fn database(&self) -> Arc<Database> {
        self.db.clone()
    }

    fn get<'a>(
        &self,
        key: <Self::KeyTableDefinition as redb::Value>::SelfType<'a>,
    ) -> Result<Option<Self::Value>, StoreError> {
        // LruCache::get requires mutable access to update LRU order
        if let Some(cache) = &self.cache
            && let Ok(mut cache_lock) = cache.blocks.lock()
            && let Some(block) = cache_lock.get(&key)
        {
            return Ok(Some(block.clone()));
        }

        let read_txn = self.database().begin_read()?;
        let table = read_txn.open_table(Self::TABLE_DEFINITION)?;
        let result = table.get(key)?;
        let block = result.map(|res| res.value());

        if let (Some(cache), Some(block)) = (&self.cache, &block)
            && let Ok(mut cache_lock) = cache.blocks.lock()
        {
            cache_lock.put(key, block.clone());
        }

        Ok(block)
    }

    fn insert(&self, key: Self::Key, value: Self::Value) -> Result<(), StoreError> {
        // insert entry to slot_index table
        let block_root = value.block.tree_hash_root();
        let slot_index_table = LeanSlotIndexTable {
            db: self.db.clone(),
        };

        slot_index_table.insert(value.block.slot, block_root)?;

        // insert entry to state root index table
        let state_root_index_table = LeanStateRootIndexTable {
            db: self.db.clone(),
        };

        state_root_index_table.insert(value.block.state_root, block_root)?;

        // insert entry to children index table, mirroring slot + parent_root so
        // fork choice can build the children map without decoding full blocks
        let children_index_table = LeanChildrenIndexTable {
            db: self.db.clone(),
        };
        children_index_table.insert(
            block_root,
            ChildIndexEntry {
                slot: value.block.slot,
                parent_root: value.block.parent_root,
            },
        )?;

        if let Some(cache) = &self.cache
            && let Ok(mut cache_lock) = cache.blocks.lock()
        {
            cache_lock.put(key, value.clone());
        }

        let mut write_txn = self.db.begin_write()?;
        write_txn.set_durability(Durability::Immediate)?;
        let mut table = write_txn.open_table(Self::TABLE_DEFINITION)?;
        table.insert(key, value)?;
        drop(table);
        write_txn.commit()?;
        Ok(())
    }

    fn remove(&self, key: Self::Key) -> Result<Option<Self::Value>, StoreError> {
        if let Some(cache) = &self.cache
            && let Ok(mut cache_lock) = cache.blocks.lock()
        {
            cache_lock.pop(&key);
        }

        let write_txn = self.db.begin_write()?;
        let mut table = write_txn.open_table(Self::TABLE_DEFINITION)?;
        let value = table.remove(key)?.map(|v| v.value());
        if let Some(block) = &value {
            let slot_index_table = LeanSlotIndexTable {
                db: self.db.clone(),
            };
            slot_index_table.remove(block.block.slot)?;
            let state_root_index_table = LeanStateRootIndexTable {
                db: self.db.clone(),
            };

            state_root_index_table.remove(block.block.state_root)?;

            let children_index_table = LeanChildrenIndexTable {
                db: self.db.clone(),
            };
            children_index_table.remove(key)?;
        }
        drop(table);
        write_txn.commit()?;
        Ok(value)
    }
}

impl LeanBlockTable {
    pub fn contains_key(&self, key: B256) -> bool {
        matches!(self.get(key), Ok(Some(_)))
    }

    /// Build the `parent_root -> children` adjacency map used by LMD GHOST.
    ///
    /// Reads the dedicated children index rather than scanning the block table,
    /// avoiding a full `SignedBlock` decode (including the large `devnet5` proof)
    /// per entry. The index is pruned on finalization, so this scan is bounded by
    /// the non-finalized block set rather than the whole chain history.
    pub fn get_children_map(
        &self,
        min_score: u64,
        attestation_weights: &HashMap<B256, u64>,
    ) -> Result<HashMap<B256, Vec<B256>>, StoreError> {
        let children_index_table = LeanChildrenIndexTable {
            db: self.db.clone(),
        };
        children_index_table.get_children_map(min_score, attestation_weights)
    }

    pub fn get_all_blocks(&self, min_slot: u64) -> Result<Vec<SignedBlock>, StoreError> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(Self::TABLE_DEFINITION)?;
        let mut blocks = Vec::new();

        for entry in table.iter()? {
            let (_, block_entry) = entry?;
            let block = block_entry.value();
            if block.block.slot >= min_slot {
                blocks.push(block);
            }
        }

        Ok(blocks)
    }
}
