pub mod beacon_state;

use std::{any::type_name, fmt::Debug};

use redb::{TypeName, Value};
use ssz::{Decode, Encode};

use crate::errors::StoreError;

#[allow(clippy::result_large_err)]
pub trait Table {
    type Key: Value;

    type Value;

    fn get(&self, key: Self::Key) -> Result<Option<Self::Value>, StoreError>;

    fn insert(&self, key: Self::Key, value: Self::Value) -> Result<(), StoreError>;
}

/// Wrapper type to handle keys and values using SSZ encoding
#[derive(Debug)]
pub struct SSZEncoding<T>(pub T);

impl<T> Value for SSZEncoding<T>
where
    T: Debug + Encode + Decode,
{
    type SelfType<'a>
        = T
    where
        Self: 'a;

    type AsBytes<'a>
        = Vec<u8>
    where
        Self: 'a;

    fn fixed_width() -> Option<usize> {
        None
    }

    fn from_bytes<'a>(data: &'a [u8]) -> Self::SelfType<'a>
    where
        Self: 'a,
    {
        Self::SelfType::from_ssz_bytes(data).expect("Failed to decode SSZ bytes, data corruption?")
    }

    fn as_bytes<'a, 'b: 'a>(value: &'a Self::SelfType<'b>) -> Self::AsBytes<'a>
    where
        Self: 'a,
        Self: 'b,
    {
        value.as_ssz_bytes()
    }

    fn type_name() -> TypeName {
        TypeName::new(&format!("SSZEncoding<{}>", type_name::<T>()))
    }
}
