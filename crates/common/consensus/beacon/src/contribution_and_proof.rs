use alloy_primitives::B256;
use ream_bls::BLSSignature;
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use ssz_types::{BitVector, typenum::U128};
use tree_hash_derive::TreeHash;

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct SyncCommitteeContribution {
    #[serde(with = "serde_utils::quoted_u64")]
    pub slot: u64,
    pub beacon_block_root: B256,
    #[serde(with = "serde_utils::quoted_u64")]
    pub subcommittee_index: u64,
    pub aggregation_bits: BitVector<U128>,
    pub signature: BLSSignature,
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct ContributionAndProof {
    #[serde(with = "serde_utils::quoted_u64")]
    pub aggregator_index: u64,
    pub contribution: SyncCommitteeContribution,
    pub selection_proof: BLSSignature,
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct SignedContributionAndProof {
    pub message: ContributionAndProof,
    pub signature: BLSSignature,
}
