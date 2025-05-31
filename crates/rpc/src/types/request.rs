use serde::Deserialize;

use crate::types::id::ValidatorID;

#[derive(Debug, Deserialize)]
pub struct ValidatorsPostRequest {
    pub ids: Option<Vec<ValidatorID>>,
    pub status: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub struct SyncCommitteeSubscription {
    pub validator_index: String,
    pub sync_committee_indices: Vec<String>,
    pub until_epoch: String,
}
