use crate::errors::StoreError;

#[allow(clippy::result_large_err)]
pub trait Field {
    type Value;

    fn get(&self) -> Result<Self::Value, StoreError>;

    fn insert(&self, value: Self::Value) -> Result<(), StoreError>;
}
