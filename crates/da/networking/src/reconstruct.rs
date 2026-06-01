use std::collections::HashSet;
use std::sync::Arc;

use alloy_primitives::B256;
use alloy_primitives::aliases::B32;
use ream_consensus_beacon::data_column_sidecar::{NUMBER_OF_COLUMNS, get_column_data_sidecars};
use ream_consensus_beacon::matrix_entry::{MatrixEntry, recover_matrix};
use ream_network_manager::p2p_sender::P2PSender;
use ream_p2p::{
    gossipsub::beacon::topics::{GossipTopic, GossipTopicKind},
    network::beacon::channel::GossipMessage,
};
use rust_eth_kzg::{DASContext, TrustedSetup, UsePrecomp};
use ssz::Encode;
use tracing::{error, info};

use ream_da_storage::DaStore;

pub async fn maybe_reconstruct(
    block_root: B256,
    store: &Arc<DaStore>,
    p2p_sender: &P2PSender,
    fork_digest: B32,
) {
    let held = match store.get_all_columns(block_root).await {
        Ok(h) => h,
        Err(e) => {
            error!(%block_root, err = %e, "Failed to load columns for reconstruction");
            return;
        }
    };

    if held.len() < 64 || held.len() == NUMBER_OF_COLUMNS as usize {
        return;
    }

    let das_context = DASContext::new(&TrustedSetup::default(), UsePrecomp::No);

    let blob_count = held[0].kzg_commitments.len() as u64;

    let partial_matrix = held
        .iter()
        .flat_map(|sidecar| {
            sidecar.column.iter().enumerate().map(|(row_index, cell)| {
                MatrixEntry::new(
                    cell.clone(),
                    sidecar.kzg_proofs[row_index],
                    sidecar.index,
                    row_index as u64,
                )
            })
        })
        .collect();

    let full_matrix = match recover_matrix(partial_matrix, blob_count, &das_context) {
        Ok(m) => m,
        Err(e) => {
            error!(%block_root, err = %e, "Reconstruction failed");
            return;
        }
    };

    let mut per_blob: Vec<(Vec<_>, Vec<_>)> = vec![(vec![], vec![]); blob_count as usize];

    for entry in &full_matrix {
        let i = entry.row_index() as usize;
        if i >= per_blob.len() {
            error!(%block_root, row_index = i, blob_count = blob_count, "row_index out of bounds");
            return;
        }
        per_blob[i].0.push(entry.cell().clone());
        per_blob[i].1.push(*entry.kzg_proof());
    }

    let signed_block_header = held[0].signed_block_header.clone();
    let kzg_commitments = held[0].kzg_commitments.clone();
    let inclusion_proof = held[0].kzg_commitments_inclusion_proof.clone();

    let all_sidecars = match get_column_data_sidecars(
        signed_block_header,
        kzg_commitments,
        inclusion_proof,
        per_blob,
    ) {
        Ok(s) => s,
        Err(e) => {
            error!(%block_root, err = ?e, "Failed to assemble recovered sidecars");
            return;
        }
    };

    let held_indices: HashSet<u64> = held.iter().map(|s| s.index).collect();

    info!(
        block_root = %block_root,
        recovered = all_sidecars.len(),
        "Reconstruction succeeded"
    );

    for sidecar in all_sidecars {
        if held_indices.contains(&sidecar.index) {
            continue;
        }

        if let Err(e) = store.put_column(block_root, sidecar.clone()).await {
            error!(%block_root, column = sidecar.index, err = ?e, "Failed to store recovered column");
            continue;
        }

        p2p_sender.send_gossip(GossipMessage {
            topic: GossipTopic {
                fork: fork_digest,
                kind: GossipTopicKind::DataColumnSidecar(sidecar.index),
            },
            data: sidecar.as_ssz_bytes(),
        });
    }
}
