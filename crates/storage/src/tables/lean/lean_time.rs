use std::sync::Arc;

use redb::{Database, Durability, ReadableDatabase, TableDefinition};

use crate::{errors::StoreError, tables::field::Field};

/// Table definition for the Lean Time table
///
/// Value: u64
pub(crate) const LEAN_TIME_FIELD: TableDefinition<&str, u64> = TableDefinition::new("lean_time");

const LEAN_TIME_KEY: &str = "lean_time_key";

pub struct LeanTimeField {
    pub db: Arc<Database>,
}

impl Field for LeanTimeField {
    type Value = u64;

    fn get(&self) -> Result<u64, StoreError> {
        let read_txn = self.db.begin_read()?;

        let table = read_txn.open_table(LEAN_TIME_FIELD)?;
        let result = table
            .get(LEAN_TIME_KEY)?
            .ok_or(StoreError::FieldNotInitilized)?;
        Ok(result.value())
    }

    fn insert(&self, value: Self::Value) -> Result<(), StoreError> {
        let mut write_txn = self.db.begin_write()?;
        write_txn.set_durability(Durability::Immediate)?;
        let mut table = write_txn.open_table(LEAN_TIME_FIELD)?;
        table.insert(LEAN_TIME_KEY, value)?;
        drop(table);
        write_txn.commit()?;
        Ok(())
    }
}
