use alloy_primitives::B256;
use serde::{Deserialize, Serialize};

use super::id::ValidatorID;
use crate::validator::ValidatorStatus;

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

#[derive(Default, Debug, Deserialize)]
pub struct BlobSidecarQuery {
    pub indices: Option<Vec<u64>>,
}

#[derive(Default, Debug, Deserialize)]
pub struct StatusQuery {
    pub status: Option<Vec<ValidatorStatus>>,
}

impl StatusQuery {
    pub fn has_status(&self) -> bool {
        match &self.status {
            Some(statuses) => !statuses.is_empty(),
            None => false,
        }
    }

    pub fn contains_status(&self, status: &ValidatorStatus) -> bool {
        match &self.status {
            Some(statuses) => statuses.contains(status),
            None => true, // If no statuses specified, accept all
        }
    }
}
