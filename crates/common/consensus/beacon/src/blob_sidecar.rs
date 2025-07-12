use alloy_primitives::B256;
use ream_consensus_misc::{
    beacon_block_header::SignedBeaconBlockHeader,
    constants::{
        BLOB_KZG_COMMITMENTS_INDEX, KZG_COMMITMENT_INCLUSION_PROOF_DEPTH, MAX_BLOBS_PER_BLOCK,
    },
};
use ream_merkle::{get_root_from_merkle_branch, is_valid_merkle_branch};
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use ssz_types::{FixedVector, typenum::U17};
use tree_hash::TreeHash;
use tree_hash_derive::TreeHash;

use crate::{
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
    Debug, Copy, Clone, PartialEq, Eq, Hash, Deserialize, Encode, Decode, Ord, PartialOrd, Default,
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
        let kzg_commitments_tree_depth =
            (MAX_BLOBS_PER_BLOCK.next_power_of_two().ilog2() + 1) as usize;

        let (kzg_commitment_to_kzg_commitments_proof, kzg_commitments_to_block_body_proof) = self
            .kzg_commitment_inclusion_proof
            .split_at(kzg_commitments_tree_depth);

        let blob_kzg_commitments_root = get_root_from_merkle_branch(
            self.kzg_commitment.tree_hash_root(),
            kzg_commitment_to_kzg_commitments_proof,
            kzg_commitments_tree_depth as u64,
            self.index,
        );

        is_valid_merkle_branch(
            blob_kzg_commitments_root,
            kzg_commitments_to_block_body_proof,
            KZG_COMMITMENT_INCLUSION_PROOF_DEPTH - kzg_commitments_tree_depth as u64,
            BLOB_KZG_COMMITMENTS_INDEX,
            self.signed_block_header.message.body_root,
        )
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use anyhow::anyhow;
    use ream_bls::BLSSignature;
    use ream_consensus_misc::beacon_block_header::{BeaconBlockHeader, SignedBeaconBlockHeader};
    use snap::raw::Decoder;
    use ssz::Decode;
    use ssz_types::{FixedVector, typenum::U17};

    use super::*;

    fn read_ssz_snappy_file<T: Decode>(path: &Path) -> anyhow::Result<T> {
        let ssz_snappy = std::fs::read(path)?;
        let mut decoder = Decoder::new();
        let ssz = decoder.decompress_vec(&ssz_snappy)?;
        T::from_ssz_bytes(&ssz).map_err(|err| anyhow!("Failed to decode SSZ: {err:?}"))
    }

    #[test]
    fn verify_blob_sidecar_inclusion_proof_positive() -> anyhow::Result<()> {
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/blob_sidecar.ssz_snappy");
        let blob_sidecar: BlobSidecar = read_ssz_snappy_file(&path)?;

        assert!(
            blob_sidecar.verify_blob_sidecar_inclusion_proof(),
            "Inclusion proof failed"
        );
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
