use alloy_primitives::{Address, B256, U256};
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use ssz_types::{
    serde_utils::{hex_fixed_vec, hex_var_list, list_of_hex_var_list},
    typenum, FixedVector, VariableList,
};
use tree_hash_derive::TreeHash;

use crate::{kzg_commitment::KZGCommitment, withdrawal::Withdrawal};

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
    pub block_number: u64,
    pub gas_limit: u64,
    pub gas_used: u64,
    pub timestamp: u64,
    #[serde(with = "hex_var_list")]
    pub extra_data: VariableList<u8, typenum::U32>,
    #[serde(with = "serde_utils::quoted_u256")]
    pub base_fee_per_gas: U256,

    pub block_hash: B256,
    #[serde(with = "list_of_hex_var_list")]
    pub transactions: VariableList<VariableList<u8, typenum::U1073741824>, typenum::U1048576>,
    pub withdrawals: VariableList<Withdrawal, typenum::U16>,
    pub blob_gas_used: u64,
    pub excess_blob_gas: u64,
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
#[serde(rename_all = "camelCase")]
pub struct BlobsBundleV1 {
    pub blobs: VariableList<KZGCommitment, typenum::U1048576>,

    #[serde(with = "list_of_hex_var_list")]
    pub commitments: VariableList<VariableList<u8, typenum::U96>, typenum::U1024>,

    #[serde(with = "list_of_hex_var_list")]
    pub proofs: VariableList<VariableList<u8, typenum::U96>, typenum::U1024>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PayloadV3 {
    pub execution_payload: ExecutionPayloadV3,
    pub block_value: B256,
    pub blobs_bundle: BlobsBundleV1,
    pub should_overide_builder: bool,
}
