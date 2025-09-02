use crate::errors::StoreError;

pub trait Table {
    type Key;

    type Value;

    fn get(&self, key: Self::Key) -> Result<Option<Self::Value>, StoreError>;

    fn insert(&self, key: Self::Key, value: Self::Value) -> Result<(), StoreError>;
}
