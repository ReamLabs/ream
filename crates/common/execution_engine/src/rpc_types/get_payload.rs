use alloy_primitives::{B256, Bytes};
use ream_consensus::polynomial_commitments::kzg_commitment::KZGCommitment;
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use ssz_types::{
    VariableList,
    serde_utils::list_of_hex_var_list,
    typenum::{U96, U1024, U1048576},
};
use tree_hash_derive::TreeHash;

use super::execution_payload::ExecutionPayloadV3;

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
#[serde(rename_all = "camelCase")]
pub struct BlobsBundleV1 {
    pub blobs: VariableList<KZGCommitment, U1048576>,
    #[serde(with = "list_of_hex_var_list")]
    pub commitments: VariableList<VariableList<u8, U96>, U1024>,
    #[serde(with = "list_of_hex_var_list")]
    pub proofs: VariableList<VariableList<u8, U96>, U1024>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PayloadV4 {
    pub execution_payload: ExecutionPayloadV3,
    pub block_value: B256,
    pub blobs_bundle: BlobsBundleV1,
    pub should_overide_builder: bool,
    pub execution_requests: Vec<Bytes>,
}
