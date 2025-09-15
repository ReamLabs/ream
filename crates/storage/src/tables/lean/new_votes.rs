use std::sync::Arc;

use ream_consensus_lean::vote::SignedVote;
use redb::{Database, Durability, TableDefinition};

use crate::{errors::StoreError, tables::ssz_encoder::SSZEncoding};
use redb::ReadableTable;

/// Table definition for the New Votes table
///
/// Key: index (u64, acts like position in an append-only array)
/// Value: `SignedVote`
pub(crate) const NEW_VOTES_TABLE: TableDefinition<u64, SSZEncoding<SignedVote>> =
    TableDefinition::new("new_votes");

pub struct NewVotesTable {
    pub db: Arc<Database>,
}

impl NewVotesTable {
    /// Append a vote to the end of the table.
    /// Returns the index at which it was inserted.
    pub fn append(&self, value: SignedVote) -> Result<(), StoreError> {
        let mut write_txn = self.db.begin_write()?;
        write_txn.set_durability(Durability::Immediate);

        let mut table = write_txn.open_table(NEW_VOTES_TABLE)?;

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

    /// Check if a given vote exists in the append-only array.
    pub fn contains(&self, value: &SignedVote) -> Result<bool, StoreError> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(NEW_VOTES_TABLE)?;

        for entry in table.iter()? {
            let (_, v) = entry?;
            if &v.value() == value {
                return Ok(true);
            }
        }

        Ok(false)
    }
}
