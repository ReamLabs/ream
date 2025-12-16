use alloy_primitives::B256;
use ream_consensus_misc::execution_requests::ExecutionRequests;
use ream_execution_rpc_types::electra::execution_payload::ExecutionPayload;
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct NewPayloadRequest {
    pub execution_payload: ExecutionPayload,
    pub versioned_hashes: Vec<B256>,
    pub parent_beacon_block_root: B256,
    pub execution_requests: ExecutionRequests,
}
