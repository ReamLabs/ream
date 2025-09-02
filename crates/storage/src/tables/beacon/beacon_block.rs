use std::sync::Arc;

use alloy_primitives::B256;
use ream_consensus_beacon::electra::beacon_block::SignedBeaconBlock;
use redb::{Database, Durability, TableDefinition};
use tree_hash::TreeHash;

use super::{
    parent_root_index::ParentRootIndexMultimapTable, slot_index::SlotIndexTable,
    state_root_index::StateRootIndexTable,
};
use crate::{
    errors::StoreError,
    tables::{multimap_table::MultimapTable, ssz_encoder::SSZEncoding, table::Table},
};

/// Table definition for the Beacon Block table
///
/// Key: block_id
/// Value: BeaconBlock
pub(crate) const BEACON_BLOCK_TABLE: TableDefinition<
    SSZEncoding<B256>,
    SSZEncoding<SignedBeaconBlock>,
> = TableDefinition::new("beacon_block");

pub struct BeaconBlockTable {
    pub db: Arc<Database>,
}

impl Table for BeaconBlockTable {
    type Key = B256;

    type Value = SignedBeaconBlock;

    fn get(&self, key: Self::Key) -> Result<Option<Self::Value>, StoreError> {
        let read_txn = self.db.begin_read()?;

        let table = read_txn.open_table(BEACON_BLOCK_TABLE)?;
        let result = table.get(key)?;
        Ok(result.map(|res| res.value()))
    }

    fn insert(&self, key: Self::Key, value: Self::Value) -> Result<(), StoreError> {
        // insert entry to slot_index table
        let block_root = value.message.tree_hash_root();
        let slot_index_table = SlotIndexTable {
            db: self.db.clone(),
        };
        slot_index_table.insert(value.message.slot, block_root)?;

        // insert entry to state root index table
        let state_root_index_table = StateRootIndexTable {
            db: self.db.clone(),
        };
        state_root_index_table.insert(value.message.state_root, block_root)?;

        let parent_root_index_table = ParentRootIndexMultimapTable {
            db: self.db.clone(),
        };
        parent_root_index_table.insert(value.message.parent_root, block_root)?;
        let mut write_txn = self.db.begin_write()?;
        write_txn.set_durability(Durability::Immediate);
        let mut table = write_txn.open_table(BEACON_BLOCK_TABLE)?;
        table.insert(key, value)?;
        drop(table);
        write_txn.commit()?;
        Ok(())
    }
}
