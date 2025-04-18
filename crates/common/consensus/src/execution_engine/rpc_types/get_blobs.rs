use alloy_primitives::FixedBytes;
use serde::Deserialize;

use crate::{constants::BYTES_PER_BLOB, polynomial_commitments::kzg_proof::KZGProof};

pub type Blob = FixedBytes<BYTES_PER_BLOB>;

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct BlobsAndProofV1 {
    pub blob: Blob,
    pub proof: KZGProof,
}
