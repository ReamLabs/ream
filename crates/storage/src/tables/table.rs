use std::{fmt::Debug, sync::Arc};

use redb::{Database, Durability, ReadableDatabase, TableDefinition};
use ssz::{Decode, Encode};

use crate::errors::StoreError;

pub trait REDBTable
where
    Self::Key: 'static,
    Self::Value: Debug + Encode + Decode + 'static,
    Self::KeyTableDefinition: redb::Key + 'static,
    Self::ValueTableDefinition: redb::Value + Debug + 'static,
    for<'a> Self::Value: From<<Self::ValueTableDefinition as redb::Value>::SelfType<'a>>,
{
    const TABLE_DEFINITION: TableDefinition<
        'static,
        Self::KeyTableDefinition,
        Self::ValueTableDefinition,
    >;

    type Key;
    type Value;
    type KeyTableDefinition;
    type ValueTableDefinition;

    fn database(&self) -> Arc<Database>;

    fn get<'a>(
        &self,
        key: <Self::KeyTableDefinition as redb::Value>::SelfType<'a>,
    ) -> Result<Option<Self::Value>, StoreError> {
        let read_txn = self.database().begin_read()?;
        let table = read_txn.open_table(Self::TABLE_DEFINITION)?;
        let result = table.get(key)?;
        Ok(result.map(|res| Self::Value::from(res.value())))
    }

    fn insert<'a>(
        &self,
        key: <Self::KeyTableDefinition as redb::Value>::SelfType<'a>,
        value: <Self::ValueTableDefinition as redb::Value>::SelfType<'a>,
    ) -> Result<(), StoreError> {
        let mut write_txn = self.database().begin_write()?;
        write_txn.set_durability(Durability::Immediate)?;
        {
            let mut table = write_txn.open_table(Self::TABLE_DEFINITION)?;
            table.insert(key, value)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    fn remove<'a>(
        &self,
        key: <Self::KeyTableDefinition as redb::Value>::SelfType<'a>,
    ) -> Result<Option<Self::Value>, StoreError> {
        let write_txn = self.database().begin_write()?;
        let value = {
            let mut table = write_txn.open_table(Self::TABLE_DEFINITION)?;
            table
                .remove(key)?
                .map(|value| Self::Value::from(value.value()))
        };
        write_txn.commit()?;
        Ok(value)
    }
}

pub trait CustomTable {
    type Key;

    type Value;

    fn get(&self, key: Self::Key) -> Result<Option<Self::Value>, StoreError>;

    fn insert(&self, key: Self::Key, value: Self::Value) -> Result<(), StoreError>;

    fn remove(&self, key: Self::Key) -> Result<Option<Self::Value>, StoreError>;
}
