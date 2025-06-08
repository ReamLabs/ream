use serde::{Deserialize, Serialize};

use crate::{id::ValidatorID, validator::ValidatorStatus};

#[derive(Debug, Deserialize, Serialize)]
pub struct ValidatorsPostRequest {
    pub ids: Option<Vec<ValidatorID>>,
    pub statuses: Option<Vec<ValidatorStatus>>,
}
