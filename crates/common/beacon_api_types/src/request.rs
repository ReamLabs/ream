use serde::{Deserialize, Serialize};

use crate::{id::ValidatorID, validator::ValidatorStatus};

#[derive(Debug, Deserialize, Serialize)]
pub struct ValidatorsPostRequest {
    pub ids: Option<Vec<ValidatorID>>,
    pub statuses: Option<Vec<ValidatorStatus>>,
}

#[derive(Debug, Deserialize)]
pub struct SyncCommitteeSubscription {
    pub validator_index: String,
    pub sync_committee_indices: Vec<String>,
    pub until_epoch: String,
}
