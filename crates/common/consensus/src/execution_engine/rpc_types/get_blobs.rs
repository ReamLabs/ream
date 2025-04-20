use alloy_consensus::Blob;
use serde::Deserialize;
use ssz_derive::{Decode, Encode};

use crate::polynomial_commitments::kzg_proof::KZGProof;

#[derive(Deserialize, Debug, Clone, PartialEq, Decode, Encode)]
#[serde(rename_all = "camelCase")]
pub struct BlobAndProofV1 {
    pub blob: Blob,
    pub proof: KZGProof,
}
