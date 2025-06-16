use alloy_primitives::U256;
use ream_bls::{BLSSignature, PubKey};
use ream_consensus::{
    electra::execution_payload_header::ExecutionPayloadHeader,
    execution_requests::ExecutionRequests, polynomial_commitments::kzg_commitment::KZGCommitment,
};
use ssz_types::{VariableList, typenum::U4096};
use tree_hash_derive::TreeHash;

#[derive(Debug, PartialEq, Eq, Clone, TreeHash)]
pub struct BuilderBid {
    pub header: ExecutionPayloadHeader,
    pub blob_kzg_commitments: VariableList<KZGCommitment, U4096>,
    pub execution_requests: ExecutionRequests,
    pub value: U256,
    pub pubkey: PubKey,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct SignedBuilderBid {
    pub message: BuilderBid,
    pub signature: BLSSignature,
}
