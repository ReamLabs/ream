use std::{collections::HashMap, sync::Arc};

use alloy_primitives::B256;
use ream_consensus_lean::block::SignedBlockWithAttestation;
use redb::{Database, Durability, ReadableDatabase, ReadableTable, TableDefinition};
use tree_hash::TreeHash;

use super::{slot_index::SlotIndexTable, state_root_index::StateRootIndexTable};
use crate::{
    errors::StoreError,
    tables::{ssz_encoder::SSZEncoding, table::Table},
};

/// Table definition for the Lean Block table
///
/// Key: block_id
/// Value: [SignedBlockWithAttestation]
pub(crate) const LEAN_BLOCK_TABLE: TableDefinition<
    SSZEncoding<B256>,
    SSZEncoding<SignedBlockWithAttestation>,
> = TableDefinition::new("lean_block");

pub struct LeanBlockTable {
    pub db: Arc<Database>,
}

impl Table for LeanBlockTable {
    type Key = B256;

    type Value = SignedBlockWithAttestation;

    fn get(&self, key: Self::Key) -> Result<Option<Self::Value>, StoreError> {
        let read_txn = self.db.begin_read()?;

        let table = read_txn.open_table(LEAN_BLOCK_TABLE)?;
        let result = table.get(key)?;
        Ok(result.map(|res| res.value()))
    }

    fn insert(&self, key: Self::Key, value: Self::Value) -> Result<(), StoreError> {
        // insert entry to slot_index table
        let block_root = value.message.block.tree_hash_root();
        let slot_index_table = SlotIndexTable {
            db: self.db.clone(),
        };
        slot_index_table.insert(value.message.block.slot, block_root)?;

        // insert entry to state root index table
        let state_root_index_table = StateRootIndexTable {
            db: self.db.clone(),
        };
        state_root_index_table.insert(value.message.block.state_root, block_root)?;

        let mut write_txn = self.db.begin_write()?;
        write_txn.set_durability(Durability::Immediate)?;
        let mut table = write_txn.open_table(LEAN_BLOCK_TABLE)?;
        table.insert(key, value)?;
        drop(table);
        write_txn.commit()?;
        Ok(())
    }

    fn remove(&self, key: Self::Key) -> Result<Option<Self::Value>, StoreError> {
        let write_txn = self.db.begin_write()?;
        let mut table = write_txn.open_table(LEAN_BLOCK_TABLE)?;
        let value = table.remove(key)?.map(|v| v.value());
        drop(table);
        write_txn.commit()?;
        Ok(value)
    }
}

impl LeanBlockTable {
    pub fn contains_key(&self, key: B256) -> bool {
        matches!(self.get(key), Ok(Some(_)))
    }

    pub fn get_children_map(
        &self,
        min_score: u64,
        attestation_weights: &HashMap<B256, u64>,
    ) -> Result<HashMap<B256, Vec<B256>>, StoreError> {
        let mut children_map = HashMap::<B256, Vec<B256>>::new();
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(LEAN_BLOCK_TABLE)?;

        for entry in table.iter()? {
            let (hash_entry, block_entry) = entry?;
            let hash: B256 = hash_entry.value();
            let block = block_entry.value().message.block;

            if block.parent_root != B256::ZERO
                && *attestation_weights.get(&hash).unwrap_or(&0) >= min_score
            {
                children_map
                    .entry(block.parent_root)
                    .or_default()
                    .push(hash);
            }
        }
        Ok(children_map)
    }
}
