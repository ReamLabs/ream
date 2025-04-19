use alloy_primitives::FixedBytes;
use serde::Deserialize;
use ssz_derive::Decode;

use crate::{constants::BYTES_PER_BLOB, polynomial_commitments::kzg_proof::KZGProof};

#[derive(Deserialize, Debug, Clone, PartialEq, Decode)]
pub struct Blob {
    pub inner: FixedBytes<BYTES_PER_BLOB>,
}

impl Blob {
    pub fn to_fixed_bytes(&self) -> [u8; BYTES_PER_BLOB] {
        let mut fixed_array = [0u8; BYTES_PER_BLOB];
        fixed_array.copy_from_slice(&*self.inner);
        fixed_array
    }
}

#[derive(Deserialize, Debug, Clone, PartialEq, Decode)]
#[serde(rename_all = "camelCase")]
pub struct BlobsAndProofV1 {
    pub blob: Blob,
    pub proof: KZGProof,
}
