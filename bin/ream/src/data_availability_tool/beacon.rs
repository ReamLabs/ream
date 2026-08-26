//! Beacon-side input: fetch a block's blobs over the Beacon API and derive the
//! block's data column sidecars locally.

use alloy_primitives::B256;
use anyhow::{Context, Result, bail, ensure};
use ream_consensus_beacon::{
    blob_sidecar::BlobSidecar,
    data_column_sidecar::{DataColumnSidecar, get_data_column_sidecars},
    matrix_entry::compute_cells_and_kzg_proofs,
};
use ream_consensus_misc::constants::beacon::DATA_COLUMN_SIDECAR_KZG_PROOF_DEPTH;
use rust_eth_kzg::{DASContext, TrustedSetup, UsePrecomp};
use serde::Deserialize;
use ssz_types::{FixedVector, VariableList};
use tree_hash::TreeHash;

/// Standard Beacon API envelope for `GET /eth/v1/beacon/blob_sidecars/{block_id}`.
#[derive(Deserialize)]
struct BlobSidecarsResponse {
    data: Vec<BlobSidecar>,
}

/// Fetch every blob sidecar the beacon node holds for `block_id`.
pub async fn fetch_blob_sidecars(beacon_url: &str, block_id: &str) -> Result<Vec<BlobSidecar>> {
    let url = format!(
        "{}/eth/v1/beacon/blob_sidecars/{block_id}",
        beacon_url.trim_end_matches('/')
    );
    let response = reqwest::Client::new()
        .get(&url)
        .header("Accept", "application/json")
        .send()
        .await
        .with_context(|| format!("GET {url}"))?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        bail!("beacon node returned {status} for {url}: {body}");
    }

    Ok(response.json::<BlobSidecarsResponse>().await?.data)
}

/// Derive all of a block's data column sidecars from its blob sidecars.
///
/// Returns `(block_root, slot, sidecars)`.
pub fn build_column_sidecars(
    mut blob_sidecars: Vec<BlobSidecar>,
) -> Result<(B256, u64, Vec<DataColumnSidecar>)> {
    ensure!(!blob_sidecars.is_empty(), "no blob sidecars to build from");

    // Blob i must line up with commitments[i], and all sidecars must belong to
    // the same block.
    blob_sidecars.sort_by_key(|sidecar| sidecar.index);
    let header = blob_sidecars[0].signed_block_header.clone();
    for (position, sidecar) in blob_sidecars.iter().enumerate() {
        ensure!(
            sidecar.index == position as u64,
            "blob indices are not contiguous: expected {position}, got {}",
            sidecar.index
        );
        ensure!(
            sidecar.signed_block_header == header,
            "blob sidecar {} belongs to a different block",
            sidecar.index
        );
    }

    let block_root = header.message.tree_hash_root();
    let slot = header.message.slot;

    let kzg_commitments = VariableList::new(
        blob_sidecars
            .iter()
            .map(|sidecar| sidecar.kzg_commitment)
            .collect(),
    )
    .map_err(|err| anyhow::anyhow!("too many commitments: {err:?}"))?;

    // The commitments-list → body-root branch: the top DEPTH nodes of the
    // per-blob inclusion proof (identical across blobs of one block).
    let blob_proof = &blob_sidecars[0].kzg_commitment_inclusion_proof;
    let top = blob_proof.len() - DATA_COLUMN_SIDECAR_KZG_PROOF_DEPTH as usize;
    let kzg_commitments_inclusion_proof = FixedVector::new(blob_proof[top..].to_vec())
        .map_err(|err| anyhow::anyhow!("bad inclusion proof length: {err:?}"))?;

    // Real KZG: extend each blob into 128 cells plus cell proofs. Loading the
    // trusted setup takes a few seconds on first use.
    let das_context = DASContext::new(&TrustedSetup::default(), UsePrecomp::No);
    let mut cells_and_kzg_proofs = Vec::with_capacity(blob_sidecars.len());
    for sidecar in &blob_sidecars {
        cells_and_kzg_proofs.push(
            compute_cells_and_kzg_proofs(&sidecar.blob, &das_context)
                .with_context(|| format!("computing cells for blob {}", sidecar.index))?,
        );
    }

    let sidecars = get_data_column_sidecars(
        header,
        kzg_commitments,
        kzg_commitments_inclusion_proof,
        cells_and_kzg_proofs,
    )
    .map_err(|err| anyhow::anyhow!("assembling column sidecars: {err:?}"))?;

    // Self-check the derived inclusion proof before submitting anything: all
    // columns share it, so verifying one is enough.
    ensure!(
        sidecars[0].verify_inclusion_proof(),
        "derived commitments inclusion proof failed verification — \
         the beacon node returned inconsistent blob sidecars"
    );

    Ok((block_root, slot, sidecars))
}
