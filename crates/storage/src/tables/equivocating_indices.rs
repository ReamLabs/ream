use std::sync::Arc;

use alloy_primitives::map::HashSet;
use redb::{Database, Durability, TableDefinition};

use super::Field;
use crate::errors::StoreError;

/// Table definition for the Equivocating_Indices table
///
/// Value: Vec<u64>
pub const EQUIVOCATING_INDICES_FIELD: TableDefinition<&str, Vec<u64>> =
    TableDefinition::new("equivocating_indices");

pub const EQUIVOCATING_INDICES_KEY: &str = "equivocating_indices_key";

pub struct EquivocatingIndicesField {
    pub db: Arc<Database>,
}

impl Field for EquivocatingIndicesField {
    type Value = HashSet<u64>;

    fn get(&self) -> Result<Option<Self::Value>, StoreError> {
        let read_txn = self.db.begin_read()?;

        let table = read_txn.open_table(EQUIVOCATING_INDICES_FIELD)?;
        let result = table.get(EQUIVOCATING_INDICES_KEY)?;
        Ok(result.map(|res| res.value().into_iter().collect()))
    }

    fn insert(&self, value: Self::Value) -> Result<(), StoreError> {
        let mut write_txn = self.db.begin_write()?;
        write_txn.set_durability(Durability::Immediate);
        let mut table = write_txn.open_table(EQUIVOCATING_INDICES_FIELD)?;
        table.insert(
            EQUIVOCATING_INDICES_KEY,
            value.into_iter().collect::<Vec<_>>(),
        )?;
        drop(table);
        write_txn.commit()?;
        Ok(())
    }
}
