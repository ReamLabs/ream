use std::sync::Arc;

use alloy_primitives::B256;
use ream_consensus_beacon::electra::beacon_state::BeaconState;
use redb::{Database, ReadableDatabase, TableDefinition};

use crate::{
    cache::BeaconCacheDB,
    errors::StoreError,
    tables::{ssz_encoder::SSZEncoding, table::REDBTable},
};

pub struct BeaconStateTable {
    pub db: Arc<Database>,
    pub cache: Option<Arc<BeaconCacheDB>>,
}

/// Table definition for the Beacon State table
///
/// Key: block_root
/// Value: BeaconState
impl REDBTable for BeaconStateTable {
    const TABLE_DEFINITION: TableDefinition<'_, SSZEncoding<B256>, SSZEncoding<BeaconState>> =
        TableDefinition::new("beacon_state");

    type Key = B256;

    type KeyTableDefinition = SSZEncoding<B256>;

    type Value = BeaconState;

    type ValueTableDefinition = SSZEncoding<BeaconState>;

    fn database(&self) -> Arc<Database> {
        self.db.clone()
    }

    fn get<'a>(
        &self,
        key: <Self::KeyTableDefinition as redb::Value>::SelfType<'a>,
    ) -> Result<Option<Self::Value>, StoreError> {
        // LruCache::get requires mutable access to update LRU order
        if let Some(cache) = &self.cache
            && let Ok(mut cache_lock) = cache.states.lock()
            && let Some(state) = cache_lock.get(&key)
        {
            return Ok(Some(state.clone()));
        }

        let read_txn = self.database().begin_read()?;
        let table = read_txn.open_table(Self::TABLE_DEFINITION)?;
        let result = table.get(key)?;
        let state = result.map(|res| res.value());

        if let (Some(cache), Some(state)) = (&self.cache, &state)
            && let Ok(mut cache_lock) = cache.states.lock()
        {
            cache_lock.put(key, state.clone());
        }

        Ok(state)
    }

    fn insert<'a>(
        &self,
        key: <Self::KeyTableDefinition as redb::Value>::SelfType<'a>,
        value: <Self::ValueTableDefinition as redb::Value>::SelfType<'a>,
    ) -> Result<(), StoreError> {
        if let Some(cache) = &self.cache
            && let Ok(mut cache_lock) = cache.states.lock()
        {
            cache_lock.put(key, value.clone());
        }

        let mut write_txn = self.database().begin_write()?;
        write_txn.set_durability(redb::Durability::Immediate)?;
        {
            let mut table = write_txn.open_table(Self::TABLE_DEFINITION)?;
            table.insert(key, value)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    fn remove<'a>(
        &self,
        key: <Self::KeyTableDefinition as redb::Value>::SelfType<'a>,
    ) -> Result<Option<Self::Value>, StoreError> {
        if let Some(cache) = &self.cache
            && let Ok(mut cache_lock) = cache.states.lock()
        {
            cache_lock.pop(&key);
        }

        let write_txn = self.database().begin_write()?;
        let value = {
            let mut table = write_txn.open_table(Self::TABLE_DEFINITION)?;
            table.remove(key)?.map(|value| value.value())
        };
        write_txn.commit()?;
        Ok(value)
    }
}
