use alloy_primitives::B256;
use anyhow::ensure;
use ream_bls::BLSSignature;
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use tree_hash::TreeHash;
use tree_hash_derive::TreeHash;

use super::beacon_block_body::BeaconBlockBody;
use crate::{
    beacon_block_header::{BeaconBlockHeader, SignedBeaconBlockHeader},
    blob_sidecar::BlobSidecar,
    execution_engine::rpc_types::get_blobs::BlobAndProofV1,
};

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct SignedBeaconBlock {
    pub message: BeaconBlock,
    pub signature: BLSSignature,
}

impl SignedBeaconBlock {
    pub fn signed_header(&self) -> SignedBeaconBlockHeader {
        SignedBeaconBlockHeader {
            message: BeaconBlockHeader {
                slot: self.message.slot,
                proposer_index: self.message.proposer_index,
                parent_root: self.message.parent_root,
                state_root: self.message.state_root,
                body_root: self.message.body.tree_hash_root(),
            },
            signature: self.signature.clone(),
        }
    }

    pub fn blob_sidecar(
        &self,
        blob_and_proof: BlobAndProofV1,
        index: u64,
    ) -> anyhow::Result<BlobSidecar> {
        ensure!(
            index < self.message.body.blob_kzg_commitments.len() as u64,
            "index must be less than the number of blob kzg commitments"
        );
        Ok(BlobSidecar {
            index,
            blob: blob_and_proof.blob,
            kzg_commitment: self.message.body.blob_kzg_commitments[index as usize],
            kzg_proof: blob_and_proof.proof,
            signed_block_header: self.signed_header(),
            kzg_commitment_inclusion_proof: self
                .message
                .body
                .blob_kzg_commitment_inclusion_proof(index)?
                .into(),
        })
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct BeaconBlock {
    #[serde(with = "serde_utils::quoted_u64")]
    pub slot: u64,
    #[serde(with = "serde_utils::quoted_u64")]
    pub proposer_index: u64,
    pub parent_root: B256,
    pub state_root: B256,
    pub body: BeaconBlockBody,
}
