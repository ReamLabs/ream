use crate::errors::StoreError;

#[allow(clippy::result_large_err)]
pub trait MultimapTable {
    type Key;

    type GetValue;

    type InsertValue;

    fn get(&self, key: Self::Key) -> Result<Option<Self::GetValue>, StoreError>;

    fn insert(&self, key: Self::Key, value: Self::InsertValue) -> Result<(), StoreError>;
}
