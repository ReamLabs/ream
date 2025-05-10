use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use ssz_types::{FixedVector, typenum::U131072};

use crate::{constants::BYTES_PER_BLOB, polynomial_commitments::kzg_proof::KZGProof};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Decode, Encode)]
#[serde(transparent)]
pub struct Blob {
    pub inner: FixedVector<u8, U131072>,
}

impl Blob {
    pub fn to_fixed_bytes(&self) -> [u8; BYTES_PER_BLOB] {
        let mut fixed_array = [0u8; BYTES_PER_BLOB];
        fixed_array.copy_from_slice(&self.inner);
        fixed_array
    }
}

#[derive(Deserialize, Debug, Clone, PartialEq, Decode, Encode)]
#[serde(rename_all = "camelCase")]
pub struct BlobAndProofV1 {
    pub blob: Blob,
    pub proof: KZGProof,
}
