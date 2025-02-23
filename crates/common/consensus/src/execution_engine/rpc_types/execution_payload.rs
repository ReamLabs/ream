use alloy_primitives::{Address, B256, U256};
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use ssz_types::{
    serde_utils::{hex_fixed_vec, hex_var_list, list_of_hex_var_list},
    typenum, FixedVector, VariableList,
};
use tree_hash_derive::TreeHash;

use crate::withdrawal::Withdrawal;

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionPayloadV3 {
    pub parent_hash: B256,
    pub fee_recipient: Address,
    pub state_root: B256,
    pub receipts_root: B256,
    #[serde(with = "hex_fixed_vec")]
    pub logs_bloom: FixedVector<u8, typenum::U256>,
    pub prev_randao: B256,
    #[serde(with = "serde_utils::u64_hex_be")]
    pub block_number: u64,
    #[serde(with = "serde_utils::u64_hex_be")]
    pub gas_limit: u64,
    #[serde(with = "serde_utils::u64_hex_be")]
    pub gas_used: u64,
    #[serde(with = "serde_utils::u64_hex_be")]
    pub timestamp: u64,
    #[serde(with = "hex_var_list")]
    pub extra_data: VariableList<u8, typenum::U32>,
    #[serde(with = "serde_utils::u256_hex_be")]
    pub base_fee_per_gas: U256,

    pub block_hash: B256,
    #[serde(with = "list_of_hex_var_list")]
    pub transactions: VariableList<VariableList<u8, typenum::U1073741824>, typenum::U1048576>,
    pub withdrawals: VariableList<Withdrawal, typenum::U16>,
    #[serde(with = "serde_utils::u64_hex_be")]
    pub blob_gas_used: u64,
    #[serde(with = "serde_utils::u64_hex_be")]
    pub excess_blob_gas: u64,
}
