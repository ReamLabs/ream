use ream_bls::BLSSignature;
use ream_consensus::attestation::Attestation;
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use tree_hash_derive::TreeHash;

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct AggregateAndProof {
    pub aggregator_index: u64,
    pub aggregate: Attestation,
    pub selection_proof: BLSSignature,
}
