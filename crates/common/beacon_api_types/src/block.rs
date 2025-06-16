use ream_consensus::{
    electra::{beacon_block::BeaconBlock, blinded_beacon_block::BlindedBeaconBlock},
    execution_engine::rpc_types::get_blobs::Blob,
    polynomial_commitments::kzg_proof::KZGProof,
};
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BroadcastValidation {
    /// Lightweight gossip checks only (default)
    Gossip,
    /// Full consensus checks, including validation of all signatures and block fields
    /// except for the execution payload transactions
    Consensus,
    /// Same as consensus, with an extra equivocation check immediately before broadcast
    ConsensusAndEquivocation,
}

impl Default for BroadcastValidation {
    fn default() -> Self {
        Self::Gossip
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProduceBlockResponse {
    pub version: String,
    pub execution_payload_blinded: bool,
    #[serde(with = "serde_utils::quoted_u64")]
    pub execution_payload_value: u64,
    #[serde(with = "serde_utils::quoted_u64")]
    pub consensus_block_value: u64,
    pub data: ProduceBlockData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ProduceBlockData {
    Full(FullBlockData),
    Blinded(BlindedBeaconBlock),
}

#[derive(Debug, Clone, Serialize, Deserialize, Encode, Decode)]
pub struct FullBlockData {
    pub block: BeaconBlock,
    pub kzg_proofs: Vec<KZGProof>,
    pub blobs: Vec<Blob>,
}
