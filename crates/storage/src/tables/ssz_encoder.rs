use std::{any::type_name, fmt::Debug};

use redb::{Key, TypeName, Value};
use ssz::{Decode, Encode};

/// Wrapper type to handle keys and values using SSZ encoding
#[derive(Debug)]
pub struct SSZEncoding<T>(pub T);

impl<T> Key for SSZEncoding<T>
where
    T: Debug + Encode + Decode + Ord,
{
    fn compare(data1: &[u8], data2: &[u8]) -> std::cmp::Ordering {
        Self::from_bytes(data1).cmp(&Self::from_bytes(data2))
    }
}

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
