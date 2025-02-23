use alloy_primitives::B256;
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use ssz_types::{serde_utils::list_of_hex_var_list, typenum, VariableList};
use tree_hash_derive::TreeHash;

use super::execution_payload::ExecutionPayloadV3;
use crate::kzg_commitment::KZGCommitment;

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
