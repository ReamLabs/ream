use std::sync::Arc;

use alloy_primitives::{B256, FixedBytes};
use redb::{Database, Durability, ReadableDatabase, TableDefinition};

use crate::{
    errors::StoreError,
    tables::{field::Field, ssz_encoder::SSZEncoding},
};

/// Table definition for the Lean_Head table
///
/// Value: B256
pub(crate) const LEAN_HEAD_FIELD: TableDefinition<&str, SSZEncoding<B256>> =
    TableDefinition::new("lean_head");

const LEAN_HEAD_KEY: &str = "lean_head_key";

pub struct LeanHeadField {
    pub db: Arc<Database>,
}

impl Field for LeanHeadField {
    type Value = B256;

    fn get(&self) -> Result<FixedBytes<32>, StoreError> {
        let read_txn = self.db.begin_read()?;

        let table = read_txn.open_table(LEAN_HEAD_FIELD)?;
        let result = table
            .get(LEAN_HEAD_KEY)?
            .ok_or(StoreError::FieldNotInitilized)?;
        Ok(result.value())
    }

    fn insert(&self, value: Self::Value) -> Result<(), StoreError> {
        let mut write_txn = self.db.begin_write()?;
        write_txn.set_durability(Durability::Immediate)?;
        let mut table = write_txn.open_table(LEAN_HEAD_FIELD)?;
        table.insert(LEAN_HEAD_KEY, value)?;
        drop(table);
        write_txn.commit()?;
        Ok(())
    }
}
