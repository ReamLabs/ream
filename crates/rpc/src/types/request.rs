use serde::Deserialize;

use crate::types::id::ValidatorID;

#[derive(Debug, Deserialize)]
pub struct ValidatorsPostRequest {
    pub ids: Option<Vec<ValidatorID>>,
    pub status: Option<Vec<String>>,
}
