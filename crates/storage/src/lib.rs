use anyhow::Result;
use parking_lot::RwLock;
use redb::{ 
    Builder,
    Database,
    Durability,
    TableDefinition
};
mod config;


pub(crate) struct ReamDB {
    db: RwLock<Database>,
}


impl ReamDB{
    fn new() -> Result<Self,redb::Error>{
        let db = Builder::new()
            .set_cache_size(config::REDB_CACHE_SIZE)
            .create(config::REDB_FILE)
            .expect("failed to create DB");

        Ok(Self {
            db 
        }) 
    }
    
    fn create_table(&self, name: &str) -> Result<()>{ 

        let table: TableDefinition<&[u8], &[u8]> = TableDefinition::new(name);
        let mut txn = self.db.begin_write()?;
        txn.set_durability(Durability::Immediate);
        txn.open_table(table);
        txn.commit().expect("failed commit to DB");
        Ok(())

    }

    fn put(&self, name: &str, key: &[u8], value: &[u8]) -> Result<()>{

        let table_def: TableDefinition<&[u8], &[u8]> = TableDefinition::new(name);
        let mut guard_db = self.db.read(); 
        let write_txn = guard_db.begin_write()?; 
        write_txn.set_durability(Durability::Immediate);
        let mut table = write_txn.open_table(table_def);
        table.insert(key, val)
            .expect("failed put commit to DB");
        drop(table);
        write_txn.commit()
        
    }

    fn get(&self, name: &str, key: &[u8]) -> Result<Option<[u8]>>{

        let table_def: TableDefinition<&[u8], &[u8]> = TableDefinition::new(name);
        let mut guard_db = self.db.read(); 
        let read_txn = guard_db.begin_write()?; 
        let table = read_txn.open_table(table_def);
        let result = table.key(key)?;
        Ok(Some(result))

    }


}

#[cfg(test)]
mod tests{
    use super::*;
    
    #[test]
    fn test_transactions() -> Result<()>{

        let ream_db = ReamDB::new().expect("error init database");
        
        let write_txnn = ream_db.db.begin_write()?;
        {
            let mut table = write_txnn.open_table(TABLE)?;
            table.insert("my_key", &123)?;
        }
        write_txnn.commit()?;

        let read_txnn = ream_db.db.begin_read()?;
        let table = read_txnn.open_table(TABLE)?;
        assert_eq!(table.get("my_key")?.unwrap().value(), 123);
        Ok(())
    }
}


