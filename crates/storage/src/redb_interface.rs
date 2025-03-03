use anyhow::Result;
use parking_lot::Mutex;
use redb::{Builder, Database, Durability, ReadableTable, TableDefinition};

use crate::{config, errors::StoreError};

pub(crate) struct ReamDB {
    db: Mutex<Database>,
}

impl ReamDB {
    fn new() -> Result<Self, StoreError> {
        let db = Builder::new()
            .set_cache_size(config::REDB_CACHE_SIZE)
            .create(config::REDB_FILE)
            .map_err(|err| StoreError::Database(err.into()))?;

        Ok(Self { db: Mutex::new(db) })
    }

    fn create_table(&self, name: &str) -> Result<(), StoreError> {
        let table: TableDefinition<'_, &[u8], &[u8]> = TableDefinition::new(name);
        let mut txn = self.db.lock().begin_write()?;
        txn.set_durability(Durability::Immediate);
        txn.open_table(table)?;
        txn.commit()?;
        Ok(())
    }

    fn put_bytes(&self, name: &str, key: &[u8], value: &[u8]) -> Result<(), StoreError> {
        let table_def: TableDefinition<'_, &[u8], &[u8]> = TableDefinition::new(name);
        let mut write_txn = self.db.lock().begin_write()?;
        write_txn.set_durability(Durability::Immediate);
        let mut table = write_txn.open_table(table_def)?;
        table.insert(key, value)?;
        drop(table);
        write_txn.commit()?;
        Ok(())
    }

    fn get_bytes(&self, name: &str, key: &[u8]) -> Result<Option<Vec<u8>>, StoreError> {
        let table_def: TableDefinition<'_, &[u8], &[u8]> = TableDefinition::new(name);
        let read_txn = self.db.lock().begin_write()?;
        let table = read_txn.open_table(table_def)?;
        let result = table.get(key)?;
        Ok(result.map(|res| res.value().to_vec()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transactions() -> Result<(), StoreError> {
        let table_def: TableDefinition<'_, &[u8], &[u8]> = TableDefinition::new("TEST");

        let test_store = ReamDB::new()?;
        let key = b"0xc424dae5e964dab6d1970424a0f3fba767762e58c59070affdc2af25e0fd6dcd";
        let val = b"0xd53f266c747ce3d59da6c6ca203ba9826ea886bc62b9191054424e9585318159";

        test_store.create_table("TEST")?;
        test_store.put_bytes("TEST", key, val);
        let result = test_store.get_bytes("TEST", key)?;

        assert_eq!(result, Some(val.to_vec()));
        Ok(())
    }
}
