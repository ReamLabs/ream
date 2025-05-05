use std::sync::Arc;

use alloy_primitives::{B256, FixedBytes};
use redb::{Database, Durability, TableDefinition};

use super::{Field, SSZEncoding};
use crate::errors::StoreError;

/// Table definition for the Proposer_Boost_Root table
///
/// Value: Root
pub const PROPOSER_BOOST_ROOT_FIELD: TableDefinition<&str, SSZEncoding<B256>> =
    TableDefinition::new("proposer_boost_root");

pub const PROPOSER_BOOST_ROOT_KEY: &str = "proposer_boost_root_key";

pub struct ProposerBoostRootField {
    pub db: Arc<Database>,
}

impl Field for ProposerBoostRootField {
    type Value = B256;

    fn get(&self) -> Result<FixedBytes<32>, StoreError> {
        let read_txn = self.db.begin_read()?;

        let table = read_txn.open_table(PROPOSER_BOOST_ROOT_FIELD)?;
        let result = table
            .get(PROPOSER_BOOST_ROOT_KEY)?
            .ok_or(StoreError::FieldNotInitilized)?;
        Ok(result.value())
    }

    fn insert(&self, value: Self::Value) -> Result<(), StoreError> {
        let mut write_txn = self.db.begin_write()?;
        write_txn.set_durability(Durability::Immediate);
        let mut table = write_txn.open_table(PROPOSER_BOOST_ROOT_FIELD)?;
        table.insert(PROPOSER_BOOST_ROOT_KEY, value)?;
        drop(table);
        write_txn.commit()?;
        Ok(())
    }
}
