use std::sync::Arc;

use alloy_primitives::B256;
use ream_consensus_beacon::electra::beacon_block::SignedBeaconBlock;
use redb::{Database, Durability, ReadableDatabase, TableDefinition};
use tree_hash::TreeHash;

use super::parent_root_index::ParentRootIndexMultimapTable;
use crate::{
    cache::BeaconCacheDB,
    errors::StoreError,
    tables::{
        beacon::{slot_index::BeaconSlotIndexTable, state_root_index::BeaconStateRootIndexTable},
        multimap_table::MultimapTable,
        ssz_encoder::SSZEncoding,
        table::REDBTable,
    },
};

pub struct BeaconBlockTable {
    pub db: Arc<Database>,
    pub cache: Option<Arc<BeaconCacheDB>>,
}

/// Table definition for the Beacon Block table
///
/// Key: block_id
/// Value: BeaconBlock
impl REDBTable for BeaconBlockTable {
    const TABLE_DEFINITION: TableDefinition<'_, SSZEncoding<B256>, SSZEncoding<SignedBeaconBlock>> =
        TableDefinition::new("beacon_block");

    type Key = B256;

    type KeyTableDefinition = SSZEncoding<B256>;

    type Value = SignedBeaconBlock;

    type ValueTableDefinition = SSZEncoding<SignedBeaconBlock>;

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
        let block_root = value.message.tree_hash_root();
        let slot_index_table = BeaconSlotIndexTable {
            db: self.db.clone(),
        };
        slot_index_table.insert(value.message.slot, block_root)?;

        // insert entry to state root index table
        let state_root_index_table = BeaconStateRootIndexTable {
            db: self.db.clone(),
        };
        state_root_index_table.insert(value.message.state_root, block_root)?;

        let parent_root_index_table = ParentRootIndexMultimapTable {
            db: self.db.clone(),
        };
        parent_root_index_table.insert(value.message.parent_root, block_root)?;

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
            let slot_index_table = BeaconSlotIndexTable {
                db: self.db.clone(),
            };
            slot_index_table.remove(block.message.slot)?;
            let state_root_index_table = BeaconStateRootIndexTable {
                db: self.db.clone(),
            };
            state_root_index_table.remove(block.message.state_root)?;
        }
        drop(table);
        write_txn.commit()?;
        Ok(value)
    }
}
