use ream_beacon_api_types::{id::ValidatorID, validator::ValidatorStatus};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidatorQuery {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ids: Option<Vec<ValidatorID>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub statuses: Option<Vec<ValidatorStatus>>,
}