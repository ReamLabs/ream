use std::{collections::HashMap, sync::Arc};

use alloy_primitives::B256;
use ream_consensus_lean::block::SignedBlockWithAttestation;
use redb::{Database, Durability, ReadableDatabase, ReadableTable, TableDefinition};
use tree_hash::TreeHash;

use super::{slot_index::LeanSlotIndexTable, state_root_index::LeanStateRootIndexTable};
use crate::{
    errors::StoreError,
    tables::{ssz_encoder::SSZEncoding, table::REDBTable},
};

pub struct LeanBlockTable {
    pub db: Arc<Database>,
}

/// Table definition for the Lean Block table
///
/// Key: block_id
/// Value: [SignedBlockWithAttestation]
impl REDBTable for LeanBlockTable {
    const TABLE_DEFINITION: TableDefinition<
        '_,
        SSZEncoding<B256>,
        SSZEncoding<SignedBlockWithAttestation>,
    > = TableDefinition::new("lean_block");

    type Key = B256;

    type KeyTableDefinition = SSZEncoding<B256>;

    type Value = SignedBlockWithAttestation;

    type ValueTableDefinition = SSZEncoding<SignedBlockWithAttestation>;

    fn database(&self) -> Arc<Database> {
        self.db.clone()
    }

    fn insert(&self, key: Self::Key, value: Self::Value) -> Result<(), StoreError> {
        // insert entry to slot_index table
        let block_root = value.message.block.tree_hash_root();
        let slot_index_table = LeanSlotIndexTable {
            db: self.db.clone(),
        };
        slot_index_table.insert(value.message.block.slot, block_root)?;

        // insert entry to state root index table
        let state_root_index_table = LeanStateRootIndexTable {
            db: self.db.clone(),
        };
        state_root_index_table.insert(value.message.block.state_root, block_root)?;

        let mut write_txn = self.db.begin_write()?;
        write_txn.set_durability(Durability::Immediate)?;
        let mut table = write_txn.open_table(Self::TABLE_DEFINITION)?;
        table.insert(key, value)?;
        drop(table);
        write_txn.commit()?;
        Ok(())
    }

    fn remove(&self, key: Self::Key) -> Result<Option<Self::Value>, StoreError> {
        let write_txn = self.db.begin_write()?;
        let mut table = write_txn.open_table(Self::TABLE_DEFINITION)?;
        let value = table.remove(key)?.map(|v| v.value());
        if let Some(block) = &value {
            let slot_index_table = LeanSlotIndexTable {
                db: self.db.clone(),
            };
            slot_index_table.remove(block.message.block.slot)?;
            let state_root_index_table = LeanStateRootIndexTable {
                db: self.db.clone(),
            };
            state_root_index_table.remove(block.message.block.state_root)?;
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

    pub fn get_children_map(
        &self,
        min_score: u64,
        attestation_weights: &HashMap<B256, u64>,
    ) -> Result<HashMap<B256, Vec<B256>>, StoreError> {
        let mut children_map = HashMap::<B256, Vec<B256>>::new();
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(Self::TABLE_DEFINITION)?;

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
