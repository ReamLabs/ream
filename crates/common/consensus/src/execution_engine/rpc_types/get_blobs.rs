use alloy_consensus::Blob;
use serde::Deserialize;
use ssz_derive::Decode;

use crate::polynomial_commitments::kzg_proof::KZGProof;

#[derive(Deserialize, Debug, Clone, PartialEq, Decode)]
#[serde(rename_all = "camelCase")]
pub struct BlobsAndProofV1 {
    pub blob: Blob,
    pub proof: KZGProof,
}
