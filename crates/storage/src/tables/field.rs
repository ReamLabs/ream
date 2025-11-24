use std::{fmt::Debug, sync::Arc};

use redb::{Database, Durability, ReadableDatabase, TableDefinition};
use ssz::{Decode, Encode};

use crate::errors::StoreError;

pub trait REDBField
where
    Self::Value: Debug + Encode + Decode + 'static,
    Self::ValueFieldDefinition: redb::Value + Debug + 'static,
    for<'a> Self::Value: From<<Self::ValueFieldDefinition as redb::Value>::SelfType<'a>>,
{
    const FIELD_DEFINITION: TableDefinition<'static, &str, Self::ValueFieldDefinition>;
    const KEY: &'static str;

    type Value;
    type ValueFieldDefinition;

    fn database(&self) -> Arc<Database>;

    fn get(&self) -> Result<Self::Value, StoreError> {
        let read_txn = self.database().begin_read()?;
        let table = read_txn.open_table(Self::FIELD_DEFINITION)?;
        let result = table
            .get(Self::KEY)?
            .ok_or(StoreError::FieldNotInitilized)?;
        Ok(Self::Value::from(result.value()))
    }

    fn insert<'a>(
        &self,
        value: <Self::ValueFieldDefinition as redb::Value>::SelfType<'a>,
    ) -> Result<(), StoreError> {
        let mut write_txn = self.database().begin_write()?;
        write_txn.set_durability(Durability::Immediate)?;
        {
            let mut table = write_txn.open_table(Self::FIELD_DEFINITION)?;
            table.insert(Self::KEY, value)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    fn remove(&self) -> Result<Option<Self::Value>, StoreError> {
        let write_txn = self.database().begin_write()?;
        let value = {
            let mut table = write_txn.open_table(Self::FIELD_DEFINITION)?;
            table
                .remove(Self::KEY)?
                .map(|v| Self::Value::from(v.value()))
        };
        write_txn.commit()?;
        Ok(value)
    }
}

pub trait CustomField {
    type Value;

    fn get(&self) -> Result<Self::Value, StoreError>;

    fn insert(&self, value: Self::Value) -> Result<(), StoreError>;

    fn remove(&self) -> Result<Option<Self::Value>, StoreError>;
}
