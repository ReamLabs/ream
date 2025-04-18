use alloy_primitives::B256;
use serde::{Deserialize, Serialize};

use super::id::ValidatorID;

#[derive(Debug, Serialize, Deserialize)]
pub struct RandaoQuery {
    pub epoch: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct SlotQuery {
    pub slot: Option<u64>,
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
