use alloy_primitives::{Bytes, U256};
use ream_consensus_misc::polynomial_commitments::kzg_commitment::KZGCommitment;
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
    // EL `blockValue` is a QUANTITY (minimal hex like "0x0"), not a 32-byte hash. It was typed
    // B256, which fails to deserialize any spec-compliant getPayloadV4 response.
    #[serde(with = "serde_utils::u256_hex_be")]
    pub block_value: U256,
    pub blobs_bundle: BlobsBundleV1,
    // Wire field is `shouldOverrideBuilder`; the Rust field name is misspelled ("overide"),
    // so the derived camelCase name (`shouldOverideBuilder`) did not match the EL response.
    #[serde(rename = "shouldOverrideBuilder")]
    pub should_overide_builder: bool,
    pub execution_requests: Vec<Bytes>,
}
