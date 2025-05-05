use alloy_primitives::B256;
use ream_consensus::{
    beacon_block_header::BeaconBlockHeader,
    electra::{beacon_block::SignedBeaconBlock, execution_payload_header::ExecutionPayloadHeader},
};
use serde::Serialize;
use ssz_types::{FixedVector, typenum::U3};
use tree_hash::TreeHash;

#[derive(Serialize)]
pub struct LightClientHeader {
    pub beacon: BeaconBlockHeader,
    pub execution: ExecutionPayloadHeader,
    pub execution_branch: FixedVector<B256, U3>,
}

impl LightClientHeader {
    pub fn new(signed_block: &SignedBeaconBlock) -> anyhow::Result<Self> {
        Ok(Self {
            beacon: BeaconBlockHeader {
                slot: signed_block.message.slot,
                proposer_index: signed_block.message.proposer_index,
                parent_root: signed_block.message.parent_root,
                state_root: signed_block.message.state_root,
                body_root: signed_block.message.body.tree_hash_root(),
            },
            execution: signed_block
                .message
                .body
                .execution_payload
                .to_execution_payload_header(),
            execution_branch: signed_block
                .message
                .body
                .execution_payload_inclusion_proof()?
                .into(),
        })
    }
}
