use ream_consensus::{
    electra::execution_payload::ExecutionPayload,
    polynomial_commitments::kzg_commitment::KZGCommitment,
};
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use ssz_types::{
    VariableList,
    typenum::{U96, U1024, U1048576},
};
use tree_hash_derive::TreeHash;

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct BlobsBundle {
    pub blobs: VariableList<KZGCommitment, U1048576>,
    pub commitments: VariableList<VariableList<u8, U96>, U1024>,
    pub proofs: VariableList<VariableList<u8, U96>, U1024>,
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct ExecutionPayloadAndBlobsBundle {
    pub execution_payload: ExecutionPayload,
    pub blobs_bundle: BlobsBundle,
}
