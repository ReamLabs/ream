use alloy_primitives::Address;
use ream_bls::PubKey;
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use tree_hash_derive::TreeHash;

use crate::misc::checksummed_address;

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct ConsolidationRequest {
    #[serde(with = "checksummed_address")]
    pub source_address: Address,
    pub source_pubkey: PubKey,
    pub target_pubkey: PubKey,
}
