use std::sync::Arc;

use anyhow::Result;
use ream_utils::dir;
use redb::{Builder, Database, Durability, TableDefinition};

use crate::{config, errors::StoreError};

pub struct ReamDB {
    db: Arc<Database>,
}

struct Connection<'a> {
    backend: &'a ReamDB,
    table: String,
}

impl<'a> Connection<'a> {
    fn put(&self, key: &[u8], value: &[u8]) -> Result<(), StoreError> {
        let table_def: TableDefinition<'_, &[u8], &[u8]> = TableDefinition::new(&self.table);
        let mut write_txn = self.backend.db.begin_write()?;

        write_txn.set_durability(Durability::Immediate);
        let mut table = write_txn.open_table(table_def)?;
        table.insert(key, value)?;
        drop(table);
        write_txn.commit()?;
        Ok(())
    }

    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>, StoreError> {
        let table_def: TableDefinition<'_, &[u8], &[u8]> = TableDefinition::new(&self.table);
        let read_txn = self.backend.db.begin_read()?;
        let table = read_txn.open_table(table_def)?;
        let result = table.get(key)?;
        Ok(result.map(|res| res.value().to_vec()))
    }
}

impl ReamDB {
    pub(crate) fn new() -> Result<Self, StoreError> {
        let ream_dir = dir::create_ream_dir().map_err(StoreError::Io)?;

        let ream_file = ream_dir.join(config::REDB_FILE);

        let db = Builder::new()
            .set_cache_size(config::REDB_CACHE_SIZE)
            .create(&ream_file)
            .map_err(|err| StoreError::Database(err.into()))?;

        Ok(Self { db: Arc::new(db) })
    }

    fn create_table(&self, name: &str) -> Result<(), StoreError> {
        let table: TableDefinition<'_, &[u8], &[u8]> = TableDefinition::new(name);
        let mut txn = self.db.begin_write()?;
        txn.set_durability(Durability::Immediate);
        txn.open_table(table)?;
        txn.commit()?;
        Ok(())
    }

    fn acquire_connection<'a>(&'a mut self, table: String) -> Result<Connection<'a>, StoreError> {
        Ok(Connection {
            backend: self,
            table,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transactions() -> Result<(), StoreError> {
        let mut test_store = ReamDB::new()?;
        test_store.create_table("TEST")?;
        let connection = test_store.acquire_connection(String::from("TEST"))?;
        let key = b"0xc424dae5e964dab6d1970424a0f3fba767762e58c59070affdc2af25e0fd6dcd";
        let val = b"0xd53f266c747ce3d59da6c6ca203ba9826ea886bc62b9191054424e9585318159";

        connection.put(key, val)?;
        let result = connection.get(key)?;

        assert_eq!(result, Some(val.to_vec()));
        Ok(())
    }
}
