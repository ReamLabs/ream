use alloy_primitives::B256;
use serde::{Deserialize, Serialize};

use super::id::ValidatorID;

macro_rules! impl_repeated_param_query {
    ($name:ident, $field:ident, $ty:ty) => {
        impl<'de> serde::Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct VisitorImpl;

                impl<'de> serde::de::Visitor<'de> for VisitorImpl {
                    type Value = $name;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                        formatter.write_str(concat!(
                            "a map with zero or more ",
                            stringify!($field),
                            " fields"
                        ))
                    }

                    fn visit_map<M>(self, mut map: M) -> Result<$name, M::Error>
                    where
                        M: serde::de::MapAccess<'de>,
                    {
                        let mut values = Vec::new();

                        while let Some(key) = map.next_key::<String>()? {
                            if key == stringify!($field) {
                                values.push(map.next_value::<$ty>()?);
                            } else {
                                let _ = map.next_value::<serde::de::IgnoredAny>()?;
                            }
                        }

                        Ok($name {
                            $field: if values.is_empty() {
                                None
                            } else {
                                Some(values)
                            },
                        })
                    }
                }

                deserializer.deserialize_map(VisitorImpl)
            }
        }
    };
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EpochQuery {
    pub epoch: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct SlotQuery {
    pub slot: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct IndexQuery {
    pub index: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct RootQuery {
    pub root: Option<B256>,
}

#[derive(Debug, Deserialize)]
pub struct ParentRootQuery {
    pub parent_root: Option<B256>,
}

#[derive(Default, Debug)]
pub struct IdQuery {
    pub id: Option<Vec<ValidatorID>>,
}

impl_repeated_param_query!(IdQuery, id, ValidatorID);

#[derive(Default, Debug)]
pub struct BlobSidecarQuery {
    pub indices: Option<Vec<u64>>,
}

impl_repeated_param_query!(BlobSidecarQuery, indices, u64);

#[derive(Default, Debug)]
pub struct StatusQuery {
    pub status: Option<Vec<String>>,
}

impl_repeated_param_query!(StatusQuery, status, String);

impl StatusQuery {
    pub fn has_status(&self) -> bool {
        match &self.status {
            Some(statuses) => !statuses.is_empty(),
            None => false,
        }
    }

    pub fn contains_status(&self, status: &String) -> bool {
        match &self.status {
            Some(statuses) => statuses.contains(status),
            None => true, // If no statuses specified, accept all
        }
    }
}
