use std::sync::Arc;

use alloy_primitives::{B256, FixedBytes};
use redb::{Database, Durability, ReadableDatabase, TableDefinition};

use crate::{
    errors::StoreError,
    tables::{field::Field, ssz_encoder::SSZEncoding},
};

/// Table definition for the Proposer_Boost_Root table
///
/// Value: Root
pub(crate) const PROPOSER_BOOST_ROOT_FIELD: TableDefinition<&str, SSZEncoding<B256>> =
    TableDefinition::new("beacon_proposer_boost_root");

const PROPOSER_BOOST_ROOT_KEY: &str = "proposer_boost_root_key";

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
        write_txn.set_durability(Durability::Immediate)?;
        let mut table = write_txn.open_table(PROPOSER_BOOST_ROOT_FIELD)?;
        table.insert(PROPOSER_BOOST_ROOT_KEY, value)?;
        drop(table);
        write_txn.commit()?;
        Ok(())
    }

    fn remove(&self) -> Result<Option<Self::Value>, StoreError> {
        let write_txn = self.db.begin_write()?;
        let mut table = write_txn.open_table(PROPOSER_BOOST_ROOT_FIELD)?;
        let value = table.remove(PROPOSER_BOOST_ROOT_KEY)?.map(|v| v.value());
        drop(table);
        write_txn.commit()?;
        Ok(value)
    }
}
