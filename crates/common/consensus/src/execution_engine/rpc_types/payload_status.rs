use alloy_primitives::B256;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Debug, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PayloadStatus {
    Valid,
    Invalid,
    Syncing,
    Accepted,
    InvalidBlockHash,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PayloadStatusV1 {
    pub status: PayloadStatus,
    pub latest_valid_hash: Option<B256>,
    pub validation_error: Option<String>,
}
