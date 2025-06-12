use alloy_primitives::B256;
use ream_bls::BLSSignature;
use serde::{Deserialize, Serialize};

use crate::{id::ValidatorID, validator::ValidatorStatus};

#[derive(Debug, Deserialize, Serialize)]
pub struct ValidatorsPostRequest {
    pub ids: Option<Vec<ValidatorID>>,
    pub statuses: Option<Vec<ValidatorStatus>>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SyncCommitteeRequestItem {
    #[serde(with = "serde_utils::quoted_u64")]
    pub slot: u64,
    pub beacon_block_root: B256,
    #[serde(with = "serde_utils::quoted_u64")]
    pub validator_index: u64,
    pub signature: BLSSignature,
}
