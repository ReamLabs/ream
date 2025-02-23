use alloy_primitives::{Address, B256, B64};
use serde::{Deserialize, Serialize};
use ssz_types::{typenum, VariableList};

use super::payload_status::PayloadStatusV1;
use crate::withdrawal::Withdrawal;

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ForkchoiceStateV1 {
    pub head_block_hash: B256,
    pub safe_block_hash: B256,
    pub finalized_block_hash: B256,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PayloadAttributesV3 {
    #[serde(with = "serde_utils::u64_hex_be")]
    pub timestamp: u64,
    pub prev_randao: B256,
    pub suggested_fee_recipient: Address,
    pub withdrawals: VariableList<Withdrawal, typenum::U16>,
    pub parent_beacon_block_root: B256,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForkchoiceUpdateResult {
    pub payload_status: PayloadStatusV1,
    pub payload_id: Option<B64>,
}
