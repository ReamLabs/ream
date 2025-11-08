use std::sync::Arc;

use alloy_primitives::{B256, FixedBytes};
use redb::{Database, Durability, ReadableDatabase, TableDefinition};

use crate::{
    errors::StoreError,
    tables::{field::Field, ssz_encoder::SSZEncoding},
};

/// Table definition for the Lean Safe Target table
///
/// Value: B256
pub(crate) const LEAN_SAFE_TARGET_FIELD: TableDefinition<&str, SSZEncoding<B256>> =
    TableDefinition::new("lean_safe_target");

const LEAN_SAFE_TARGET_KEY: &str = "lean_safe_target_key";

pub struct LeanSafeTargetField {
    pub db: Arc<Database>,
}

impl Field for LeanSafeTargetField {
    type Value = B256;

    fn get(&self) -> Result<FixedBytes<32>, StoreError> {
        let read_txn = self.db.begin_read()?;

        let table = read_txn.open_table(LEAN_SAFE_TARGET_FIELD)?;
        let result = table
            .get(LEAN_SAFE_TARGET_KEY)?
            .ok_or(StoreError::FieldNotInitilized)?;
        Ok(result.value())
    }

    fn insert(&self, value: Self::Value) -> Result<(), StoreError> {
        let mut write_txn = self.db.begin_write()?;
        write_txn.set_durability(Durability::Immediate)?;
        let mut table = write_txn.open_table(LEAN_SAFE_TARGET_FIELD)?;
        table.insert(LEAN_SAFE_TARGET_KEY, value)?;
        drop(table);
        write_txn.commit()?;
        Ok(())
    }

    fn remove(&self) -> Result<Option<Self::Value>, StoreError> {
        let write_txn = self.db.begin_write()?;
        let mut table = write_txn.open_table(LEAN_SAFE_TARGET_FIELD)?;
        let value = table.remove(LEAN_SAFE_TARGET_KEY)?.map(|v| v.value());
        drop(table);
        write_txn.commit()?;
        Ok(value)
    }
}
