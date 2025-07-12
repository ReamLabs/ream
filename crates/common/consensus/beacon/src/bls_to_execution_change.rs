use alloy_primitives::Address;
use ream_bls::{BLSSignature, PublicKey};
use ream_consensus_misc::misc::checksummed_address;
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use tree_hash_derive::TreeHash;

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct SignedBLSToExecutionChange {
    pub message: BLSToExecutionChange,
    pub signature: BLSSignature,
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct BLSToExecutionChange {
    #[serde(with = "serde_utils::quoted_u64")]
    pub validator_index: u64,
    #[serde(rename = "from_bls_pubkey")]
    pub from_bls_public_key: PublicKey,
    #[serde(with = "checksummed_address")]
    pub to_execution_address: Address,
}
