use alloy_primitives::B256;
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use ssz_types::{typenum::U33, FixedVector};
use tree_hash_derive::TreeHash;

use crate::deposit_data::DepositData;

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct Deposit {
    pub proof: FixedVector<B256, U33>,
    pub data: DepositData,
}
