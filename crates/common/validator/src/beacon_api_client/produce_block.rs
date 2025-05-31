use ream_consensus::{
    electra::beacon_block::BeaconBlock, execution_engine::rpc_types::get_blobs::Blob,
    polynomial_commitments::kzg_proof::KZGProof,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProduceBlock {
    pub version: String,
    pub execution_payload_blinded: bool,
    #[serde(with = "serde_utils::quoted_u64")]
    pub execution_payload_value: u64,
    #[serde(with = "serde_utils::quoted_u64")]
    pub consensus_block_value: u64,
    pub data: ProduceBlockData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProduceBlockData {
    pub block: BeaconBlock,
    pub kzg_proofs: Vec<KZGProof>,
    pub blobs: Vec<Blob>,
}
