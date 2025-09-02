use std::sync::Arc;

use redb::{Database, Durability, TableDefinition};

use crate::{errors::StoreError, tables::field::Field};

/// Table definition for the Time table
///
/// Value: u64
pub(crate) const TIME_FIELD: TableDefinition<&str, u64> = TableDefinition::new("beacon_time");

const TIME_KEY: &str = "time_key";

pub struct TimeField {
    pub db: Arc<Database>,
}

impl Field for TimeField {
    type Value = u64;

    fn get(&self) -> Result<u64, StoreError> {
        let read_txn = self.db.begin_read()?;

        let table = read_txn.open_table(TIME_FIELD)?;
        let result = table.get(TIME_KEY)?.ok_or(StoreError::FieldNotInitilized)?;
        Ok(result.value())
    }

    fn insert(&self, value: Self::Value) -> Result<(), StoreError> {
        let mut write_txn = self.db.begin_write()?;
        write_txn.set_durability(Durability::Immediate);
        let mut table = write_txn.open_table(TIME_FIELD)?;
        table.insert(TIME_KEY, value)?;
        drop(table);
        write_txn.commit()?;
        Ok(())
    }
}
