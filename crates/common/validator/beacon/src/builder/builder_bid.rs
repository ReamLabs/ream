use alloy_primitives::U256;
use ream_bls::{BLSSignature, PublicKey};
use ream_consensus_misc::{
    execution_requests::ExecutionRequests, polynomial_commitments::kzg_commitment::KZGCommitment,
};
use ream_execution_rpc_types::electra::execution_payload_header::ExecutionPayloadHeader;
use serde::{Deserialize, Serialize};
use ssz_types::{VariableList, typenum::U4096};
use tree_hash_derive::TreeHash;

#[derive(Debug, PartialEq, Eq, Clone, TreeHash, Serialize, Deserialize)]
pub struct BuilderBid {
    pub header: ExecutionPayloadHeader,
    pub blob_kzg_commitments: VariableList<KZGCommitment, U4096>,
    pub execution_requests: ExecutionRequests,
    pub value: U256,
    #[serde(rename = "pubkey")]
    pub public_key: PublicKey,
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct SignedBuilderBid {
    pub message: BuilderBid,
    pub signature: BLSSignature,
}
