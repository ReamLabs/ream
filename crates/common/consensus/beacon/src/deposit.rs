use alloy_primitives::B256;
use ream_consensus_misc::deposit_data::DepositData;
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use ssz_types::{FixedVector, typenum::U33};
use tree_hash_derive::TreeHash;

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct Deposit {
    pub proof: FixedVector<B256, U33>,
    pub data: DepositData,
}
