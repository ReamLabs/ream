use alloy_primitives::B256;
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};

use crate::deneb::execution_payload::ExecutionPayload;

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct NewPayloadRequest {
    pub execution_payload: ExecutionPayload,
    pub versioned_hashes: Vec<B256>,
    pub parent_beacon_block_root: B256,
}
