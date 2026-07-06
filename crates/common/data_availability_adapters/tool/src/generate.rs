//! Offline input: synthesize KZG-valid `DataColumnSidecar`s with no beacon
//! node in sight, for exercising the data availability pipeline's success path anywhere.
//!
//! Ported from the `feat/da-client` branch's `generate_sidecars` example. The
//! data node deliberately does not verify proposer signatures or block
//! authenticity (those need beacon state), so the commitments inclusion proof
//! does not need a real block body: we build a 16-leaf merkle tree with the
//! commitments root at leaf `BLOB_KZG_COMMITMENTS_INDEX` and use the tree root
//! as the header's `body_root`. The KZG cells and cell proofs are real,
//! computed from all-zero blobs whose commitment is the G1 point at infinity.

use alloy_primitives::B256;
use anyhow::{Context, Result, ensure};
use ream_consensus_beacon::{
    data_column_sidecar::{DataColumnSidecar, get_data_column_sidecars},
    matrix_entry::compute_cells_and_kzg_proofs,
};
use ream_consensus_misc::{
    beacon_block_header::{BeaconBlockHeader, SignedBeaconBlockHeader},
    constants::beacon::{BLOB_KZG_COMMITMENTS_INDEX, DATA_COLUMN_SIDECAR_KZG_PROOF_DEPTH},
    polynomial_commitments::kzg_commitment::KZGCommitment,
};
use ream_execution_rpc_types::get_blobs::Blob;
use ream_merkle::{generate_proof, merkle_tree};
use rust_eth_kzg::{DASContext, TrustedSetup, UsePrecomp};
use ssz_types::{FixedVector, VariableList};
use tree_hash::TreeHash;

/// Build a synthetic block's worth of KZG-valid column sidecars: `blobs`
/// all-zero blobs under a self-consistent header at `slot`.
///
/// Deterministic: the same `(slot, blobs)` always yields the same block root,
/// so vectors can be regenerated and availability re-queried across runs.
///
/// Returns `(block_root, slot, sidecars)`.
pub fn build_synthetic_sidecars(
    slot: u64,
    blobs: usize,
) -> Result<(B256, u64, Vec<DataColumnSidecar>)> {
    ensure!(blobs > 0, "a synthetic block needs at least one blob");

    // Every blob is all-zero (the zero polynomial), so one real KZG
    // computation covers them all…
    let blob = Blob {
        inner: FixedVector::default(),
    };
    let das_context = DASContext::new(&TrustedSetup::default(), UsePrecomp::No);
    let (cells, proofs) =
        compute_cells_and_kzg_proofs(&blob, &das_context).context("computing cells and proofs")?;
    let cells_and_kzg_proofs = vec![(cells, proofs); blobs];

    // …and they all share the zero polynomial's commitment: the G1 point at
    // infinity (compressed encoding 0xc0 followed by zeros).
    let mut commitment_bytes = [0u8; 48];
    commitment_bytes[0] = 0xc0;
    let kzg_commitments = VariableList::new(vec![KZGCommitment(commitment_bytes); blobs])
        .map_err(|err| anyhow::anyhow!("too many blobs: {err:?}"))?;

    // Self-built inclusion proof: a tree of
    // 2^DATA_COLUMN_SIDECAR_KZG_PROOF_DEPTH leaves with the commitments root at
    // leaf BLOB_KZG_COMMITMENTS_INDEX; the tree root becomes the header's
    // body_root, so the branch verifies.
    let mut leaves = vec![B256::ZERO; 1 << DATA_COLUMN_SIDECAR_KZG_PROOF_DEPTH];
    leaves[BLOB_KZG_COMMITMENTS_INDEX as usize] = kzg_commitments.tree_hash_root();
    let tree = merkle_tree(&leaves, DATA_COLUMN_SIDECAR_KZG_PROOF_DEPTH)?;
    let inclusion_proof = FixedVector::new(generate_proof(
        &tree,
        BLOB_KZG_COMMITMENTS_INDEX,
        DATA_COLUMN_SIDECAR_KZG_PROOF_DEPTH,
    )?)
    .map_err(|err| anyhow::anyhow!("proof length mismatch: {err:?}"))?;
    let body_root = tree[1];

    // A zeroed signature is fine: the data node does not verify proposer
    // signatures (that requires beacon state).
    let signed_block_header = SignedBeaconBlockHeader {
        message: BeaconBlockHeader {
            slot,
            proposer_index: 0,
            parent_root: B256::ZERO,
            state_root: B256::ZERO,
            body_root,
        },
        signature: Default::default(),
    };

    let block_root = signed_block_header.message.tree_hash_root();
    let sidecars = get_data_column_sidecars(
        signed_block_header,
        kzg_commitments,
        inclusion_proof,
        cells_and_kzg_proofs,
    )
    .map_err(|err| anyhow::anyhow!("assembling column sidecars: {err:?}"))?;

    Ok((block_root, slot, sidecars))
}
