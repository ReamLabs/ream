use std::fmt;

use alloy_primitives::B256;
use serde::{
    Deserialize, Deserializer, Serialize,
    de::{MapAccess, Visitor},
};

use super::id::ValidatorID;

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

#[derive(Default, Debug, Deserialize)]
pub struct IdQuery {
    pub id: Option<Vec<ValidatorID>>,
}

#[derive(Default, Debug)]
pub struct BlobSidecarQuery {
    pub indices: Option<Vec<u64>>,
}

#[derive(Default, Debug, Deserialize)]
pub struct StatusQuery {
    pub status: Option<Vec<String>>,
}

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

impl<'de> Deserialize<'de> for BlobSidecarQuery {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct BlobSidecarQueryVisitor;

        impl<'de> Visitor<'de> for BlobSidecarQueryVisitor {
            type Value = BlobSidecarQuery;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a map with zero or more indices fields")
            }

            fn visit_map<M>(self, mut map: M) -> Result<BlobSidecarQuery, M::Error>
            where
                M: MapAccess<'de>,
            {
                let mut indices = Vec::new();

                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "indices" => {
                            indices.push(map.next_value()?);
                        }
                        _ => {
                            let _ = map.next_value::<serde::de::IgnoredAny>()?;
                        }
                    }
                }

                Ok(BlobSidecarQuery {
                    indices: if indices.is_empty() {
                        None
                    } else {
                        Some(indices)
                    },
                })
            }
        }

        deserializer.deserialize_map(BlobSidecarQueryVisitor)
    }
}
