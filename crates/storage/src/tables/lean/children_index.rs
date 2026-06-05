use std::{collections::HashMap, sync::Arc};

use alloy_primitives::B256;
use redb::{Database, Durability, ReadableDatabase, ReadableTable, TableDefinition};
use ssz_derive::{Decode, Encode};

use crate::{
    errors::StoreError,
    tables::{ssz_encoder::SSZEncoding, table::REDBTable},
};

/// Value stored in the children index for a single block.
///
/// The children index mirrors `parent_root` (and `slot`, used for pruning) out
/// of the block table so that fork choice can build the parent -> children
/// adjacency without deserializing full [`SignedBlock`](ream_consensus_lean::block::SignedBlock)
/// values, whose `devnet5` proof field can reach hundreds of kilobytes.
#[derive(Debug, PartialEq, Eq, Clone, Encode, Decode)]
pub struct ChildIndexEntry {
    pub slot: u64,
    pub parent_root: B256,
}

pub struct LeanChildrenIndexTable {
    pub db: Arc<Database>,
}

/// Table definition for the Children Index table
///
/// Key: block_root
/// Value: [ChildIndexEntry] (the block's slot and parent_root)
impl REDBTable for LeanChildrenIndexTable {
    const TABLE_DEFINITION: TableDefinition<'_, SSZEncoding<B256>, SSZEncoding<ChildIndexEntry>> =
        TableDefinition::new("lean_children_index");

    type Key = B256;

    type KeyTableDefinition = SSZEncoding<B256>;

    type Value = ChildIndexEntry;

    type ValueTableDefinition = SSZEncoding<ChildIndexEntry>;

    fn database(&self) -> Arc<Database> {
        self.db.clone()
    }
}

impl LeanChildrenIndexTable {
    /// Build the `parent_root -> children` adjacency map used by LMD GHOST.
    ///
    /// Iterates this index instead of the block table, so each entry costs a
    /// ~40 byte decode rather than a full `SignedBlock` decode. Because the
    /// index is pruned on finalization (see [`Self::prune_finalized`]), the scan
    /// stays bounded by the non-finalized block set rather than growing with the
    /// whole chain history.
    pub fn get_children_map(
        &self,
        min_score: u64,
        attestation_weights: &HashMap<B256, u64>,
    ) -> Result<HashMap<B256, Vec<B256>>, StoreError> {
        let mut children_map = HashMap::<B256, Vec<B256>>::new();
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(Self::TABLE_DEFINITION)?;

        for entry in table.iter()? {
            let (root_entry, value_entry) = entry?;
            let root: B256 = root_entry.value();
            let parent_root = value_entry.value().parent_root;

            if parent_root == B256::ZERO {
                continue;
            }

            if min_score > 0 && attestation_weights.get(&root).unwrap_or(&0) < &min_score {
                continue;
            }

            children_map.entry(parent_root).or_default().push(root);
        }
        Ok(children_map)
    }

    /// Remove index entries for blocks below `finalized_slot`.
    pub fn prune_finalized(&self, finalized_slot: u64) -> Result<usize, StoreError> {
        let stale_roots: Vec<B256> = {
            let read_txn = self.db.begin_read()?;
            let table = read_txn.open_table(Self::TABLE_DEFINITION)?;
            let mut roots = Vec::new();
            for entry in table.iter()? {
                let (root_entry, value_entry) = entry?;
                if value_entry.value().slot < finalized_slot {
                    roots.push(root_entry.value());
                }
            }
            roots
        };

        if stale_roots.is_empty() {
            return Ok(0);
        }

        let mut write_txn = self.db.begin_write()?;
        write_txn.set_durability(Durability::Immediate)?;
        {
            let mut table = write_txn.open_table(Self::TABLE_DEFINITION)?;
            for root in &stale_roots {
                table.remove(*root)?;
            }
        }
        write_txn.commit()?;
        Ok(stale_roots.len())
    }
}
