use serde::{Deserialize, Serialize};

use crate::{id::ValidatorID, validator::ValidatorStatus};

#[derive(Debug, Deserialize, Serialize)]
pub struct ValidatorsPostRequest {
    pub ids: Option<Vec<ValidatorID>>,
    pub statuses: Option<Vec<ValidatorStatus>>,
}

#[derive(Debug, Deserialize)]
pub struct SyncCommitteeSubscription {
    pub validator_index: u64,
    pub sync_committee_indices: Vec<u64>,
    pub until_epoch: u64,
}
