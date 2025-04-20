use std::sync::Arc;

use redb::{Database, Durability, TableDefinition};

use super::Field;
use crate::errors::StoreError;

/// Table definition for the Genesis_Time table
///
/// Value: u64
pub const GENESIS_TIME_FIELD: TableDefinition<&str, u64> = TableDefinition::new("genesis_time");

pub const GENESIS_TIME_KEY: &str = "genesis_time_key";

pub struct GenesisTimeField {
    pub db: Arc<Database>,
}

impl Field for GenesisTimeField {
    type Value = u64;

    fn get(&self) -> Result<u64, StoreError> {
        let read_txn = self.db.begin_read()?;

        let table = read_txn.open_table(GENESIS_TIME_FIELD)?;
        let result = table
            .get(GENESIS_TIME_KEY)?
            .ok_or(StoreError::FieldNotInitilized)?;
        Ok(result.value())
    }

    fn insert(&self, value: Self::Value) -> Result<(), StoreError> {
        let mut write_txn = self.db.begin_write()?;
        write_txn.set_durability(Durability::Immediate);
        let mut table = write_txn.open_table(GENESIS_TIME_FIELD)?;
        table.insert(GENESIS_TIME_KEY, value)?;
        drop(table);
        write_txn.commit()?;
        Ok(())
    }
}
