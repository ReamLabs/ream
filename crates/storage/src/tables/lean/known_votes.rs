use std::sync::Arc;

use alloy_primitives::B256;
use ream_consensus_lean::{block::SignedBlock, vote::SignedVote};
use redb::{Database, Durability, ReadableTable, ReadableTableMetadata, TableDefinition};

use crate::{errors::StoreError, tables::ssz_encoder::SSZEncoding};

/// Table definition for the Known Votes table
///
/// Key: index (u64, acts like position in an append-only array)
/// Value: `SignedVote`
pub(crate) const KNOWN_VOTES_TABLE: TableDefinition<u64, SSZEncoding<SignedVote>> =
    TableDefinition::new("known_votes");

pub struct KnownVotesTable {
    pub db: Arc<Database>,
}

impl KnownVotesTable {
    /// Append a vote to the end of the table.
    /// Returns the index at which it was inserted.
    pub fn append(&self, value: SignedVote) -> Result<(), StoreError> {
        let mut write_txn = self.db.begin_write()?;
        write_txn.set_durability(Durability::Immediate);

        let mut table = write_txn.open_table(KNOWN_VOTES_TABLE)?;

        // Compute next index
        let next_index = match table.last()? {
            Some((k, _)) => k.value() + 1,
            None => 0,
        };

        table.insert(next_index, value)?;

        drop(table);
        write_txn.commit()?;
        Ok(())
    }

    /// Append multiple votes in a single transaction.
    /// Returns the starting index of the first inserted vote.
    pub fn batch_append(
        &self,
        values: impl IntoIterator<Item = SignedVote>,
    ) -> Result<u64, StoreError> {
        let mut write_txn = self.db.begin_write()?;
        write_txn.set_durability(Durability::Immediate);

        let mut table = write_txn.open_table(KNOWN_VOTES_TABLE)?;

        // Find the next free index
        let mut next_index = match table.last()? {
            Some((k, _)) => k.value() + 1,
            None => 0,
        };

        let start_index = next_index;

        for value in values {
            table.insert(next_index, value)?;
            next_index += 1;
        }

        drop(table);
        write_txn.commit()?;

        Ok(start_index)
    }

    /// Check if a given vote exists in the append-only array.
    pub fn contains(&self, value: &SignedVote) -> Result<bool, StoreError> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(KNOWN_VOTES_TABLE)?;

        for entry in table.iter()? {
            let (_, v) = entry?;
            if &v.value() == value {
                return Ok(true);
            }
        }

        Ok(false)
    }

    /// Return number of votes (like `Vec::len`)
    pub fn len(&self) -> Result<u64, StoreError> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(KNOWN_VOTES_TABLE)?;
        Ok(table.len()?)
    }

    /// Returns if there are no known votes
    pub fn is_empty(&self) -> Result<bool, StoreError> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(KNOWN_VOTES_TABLE)?;
        Ok(table.len()? == 0)
    }

    /// Get all votes.
    pub fn get_all_votes(&self) -> Result<Vec<SignedVote>, StoreError> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(KNOWN_VOTES_TABLE)?;

        let mut votes = Vec::with_capacity(table.len()? as usize);

        for entry in table.iter()? {
            let (_, v) = entry?;
            votes.push(v.value());
        }

        Ok(votes)
    }

    /// Get all votes whose `source.root` matches `state.latest_justified.root`
    /// and that are not already in the block's attestations.
    pub fn filter_new_votes_to_add(
        &self,
        justified_root: B256,
        new_block: &SignedBlock,
    ) -> Result<Vec<SignedVote>, StoreError> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(KNOWN_VOTES_TABLE)?;

        let mut result = Vec::new();

        for entry in table.iter()? {
            let (_, v) = entry?;
            let vote = v.value();

            if vote.message.source.root == justified_root
                && !new_block.message.body.attestations.contains(&vote)
            {
                result.push(vote);
            }
        }

        Ok(result)
    }
}
