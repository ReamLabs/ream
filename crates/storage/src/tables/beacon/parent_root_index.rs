use std::sync::Arc;

use alloy_primitives::B256;
use redb::{Database, Durability, MultimapTableDefinition};

use crate::{
    errors::StoreError,
    tables::{multimap_table::MultimapTable, ssz_encoder::SSZEncoding},
};

/// Table definition for the Parent Root Index Multimap table
///
/// Key: ParentRoot
/// Value: BlockRoot's
pub(crate) const PARENT_ROOT_INDEX_MULTIMAP_TABLE: MultimapTableDefinition<
    SSZEncoding<B256>,
    SSZEncoding<B256>,
> = MultimapTableDefinition::new("beacon_parent_root_index_multimap");

pub struct ParentRootIndexMultimapTable {
    pub db: Arc<Database>,
}

impl MultimapTable for ParentRootIndexMultimapTable {
    type Key = B256;

    type GetValue = Vec<B256>;

    type InsertValue = B256;

    fn get(&self, key: Self::Key) -> Result<Option<Self::GetValue>, StoreError> {
        let read_txn = self.db.begin_read()?;

        let table = read_txn.open_multimap_table(PARENT_ROOT_INDEX_MULTIMAP_TABLE)?;
        let result = table.get(key)?;
        let mut values = vec![];
        for value in result {
            values.push(value?.value());
        }
        Ok(Some(values))
    }

    fn insert(&self, key: Self::Key, value: Self::InsertValue) -> Result<(), StoreError> {
        let mut write_txn = self.db.begin_write()?;
        write_txn.set_durability(Durability::Immediate);
        let mut table = write_txn.open_multimap_table(PARENT_ROOT_INDEX_MULTIMAP_TABLE)?;
        table.insert(key, value)?;
        drop(table);
        write_txn.commit()?;
        Ok(())
    }
}
