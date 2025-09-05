use std::sync::Arc;

use alloy_primitives::B256;
use redb::{Database, Durability, TableDefinition};

use crate::{
    errors::StoreError,
    tables::{field::Field, ssz_encoder::SSZEncoding},
};

/// Table definition for the Finalized_Checkpoint table
///
/// Value: block root
pub(crate) const SAFE_TARGET_FIELD: TableDefinition<&str, SSZEncoding<B256>> =
    TableDefinition::new("lean_safe_target");

const SAFE_TARGET_FIELD_KEY: &str = "safe_target_block";

pub struct SafeTargetField {
    pub db: Arc<Database>,
}

impl Field for SafeTargetField {
    type Value = B256;

    fn get(&self) -> Result<B256, StoreError> {
        let read_txn = self.db.begin_read()?;

        let table = read_txn.open_table(SAFE_TARGET_FIELD)?;
        let result = table
            .get(SAFE_TARGET_FIELD_KEY)?
            .ok_or(StoreError::FieldNotInitilized)?;
        Ok(result.value())
    }

    fn insert(&self, value: Self::Value) -> Result<(), StoreError> {
        let mut write_txn = self.db.begin_write()?;
        write_txn.set_durability(Durability::Immediate);
        let mut table = write_txn.open_table(SAFE_TARGET_FIELD)?;
        table.insert(SAFE_TARGET_FIELD_KEY, value)?;
        drop(table);
        write_txn.commit()?;
        Ok(())
    }
}
