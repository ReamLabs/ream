use alloy_primitives::B256;
use ream_merkle::is_valid_merkle_branch;
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use ssz_types::{
    FixedVector,
    typenum::{U17, Unsigned},
};
use tree_hash::TreeHash;
use tree_hash_derive::TreeHash;

use crate::{
    beacon_block_header::SignedBeaconBlockHeader,
    constants::BLOB_KZG_COMMITMENTS_INDEX,
    execution_engine::rpc_types::get_blobs::Blob,
    polynomial_commitments::{kzg_commitment::KZGCommitment, kzg_proof::KZGProof},
};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, Encode, Decode, TreeHash)]
pub struct BlobSidecar {
    #[serde(with = "serde_utils::quoted_u64")]
    pub index: u64,
    pub blob: Blob,
    pub kzg_commitment: KZGCommitment,
    pub kzg_proof: KZGProof,
    pub signed_block_header: SignedBeaconBlockHeader,
    pub kzg_commitment_inclusion_proof: FixedVector<B256, U17>,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Hash, Deserialize, Encode, Decode, Ord, PartialOrd, Default,
)]
pub struct BlobIdentifier {
    pub block_root: B256,
    pub index: u64,
}

impl BlobIdentifier {
    pub fn new(block_root: B256, index: u64) -> Self {
        Self { block_root, index }
    }
}

impl BlobSidecar {
    pub fn verify_blob_sidecar_inclusion_proof(&self) -> bool {
        is_valid_merkle_branch(
            self.kzg_commitment.tree_hash_root(),
            &self.kzg_commitment_inclusion_proof,
            U17::USIZE as u64,
            BLOB_KZG_COMMITMENTS_INDEX,
            self.signed_block_header.message.body_root,
        )
    }
}

#[cfg(test)]
mod tests {
    use alloy_primitives::B256;
    use ream_bls::BLSSignature;
    use ream_merkle::{generate_proof, merkle_tree};
    use ssz_types::{FixedVector, typenum::U17};
    use tree_hash::TreeHash;

    use crate::{
        beacon_block_header::{BeaconBlockHeader, SignedBeaconBlockHeader},
        blob_sidecar::BlobSidecar,
        constants::BLOB_KZG_COMMITMENTS_INDEX,
        execution_engine::rpc_types::get_blobs::Blob,
        polynomial_commitments::{kzg_commitment::KZGCommitment, kzg_proof::KZGProof},
    };

    #[test]
    fn verify_blob_sidecar_inclusion_proof_positive() -> anyhow::Result<()> {
        let blob = Blob {
            inner: FixedVector::from(vec![0x42; 131072]),
        };

        let kzg_commitment = KZGCommitment([0u8; 48]);
        let depth = 17;

        let mut leaves = vec![B256::default(); 1 << depth];
        leaves[BLOB_KZG_COMMITMENTS_INDEX as usize] = kzg_commitment.tree_hash_root();

        let tree = merkle_tree(&leaves, depth)?;
        let root = tree[1];

        let mut proof = generate_proof(&tree, BLOB_KZG_COMMITMENTS_INDEX, depth)?;
        proof.resize(17, B256::default());

        let blob_sidecar = BlobSidecar {
            index: BLOB_KZG_COMMITMENTS_INDEX,
            blob,
            kzg_commitment,
            kzg_proof: KZGProof::default(),
            signed_block_header: SignedBeaconBlockHeader {
                message: BeaconBlockHeader {
                    body_root: root,
                    ..Default::default()
                },
                signature: BLSSignature::default(),
            },
            kzg_commitment_inclusion_proof: FixedVector::from(proof),
        };

        assert!(blob_sidecar.verify_blob_sidecar_inclusion_proof());
        Ok(())
    }

    #[test]
    fn verify_blob_sidecar_inclusion_proof_negative() -> anyhow::Result<()> {
        let signed_block_header = SignedBeaconBlockHeader {
            message: BeaconBlockHeader::default(),
            signature: BLSSignature::default(),
        };

        let blob_sidecar = BlobSidecar {
            index: u64::default(),
            blob: Blob::default(),
            kzg_commitment: KZGCommitment([0u8; 48]),
            kzg_proof: KZGProof::default(),
            signed_block_header,
            kzg_commitment_inclusion_proof: FixedVector::<B256, U17>::from(vec![
                B256::default();
                17
            ]),
        };

        let result = blob_sidecar.verify_blob_sidecar_inclusion_proof();

        assert!(!result, "Expected verification to fail");

        Ok(())
    }
}
