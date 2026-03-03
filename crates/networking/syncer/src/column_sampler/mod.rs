use std::{collections::HashMap, sync::Arc};

use alloy_primitives::B256;
use libp2p::PeerId;
use rand::{Rng, SeedableRng, seq::index::sample};
use ream_consensus_beacon::{
    data_column_sidecar::{DataColumnSidecar, NUMBER_OF_COLUMNS},
    peer_sampling::get_extended_sample_count,
};
use ream_consensus_misc::constants::beacon::SAMPLES_PER_SLOT;
use ream_executor::ReamExecutor;
use ream_p2p::network::beacon::{
    channel::{P2PCallbackResponse, P2PMessage, P2PRequest},
    network_state::NetworkState,
};
use ream_req_resp::beacon::messages::{
    BeaconResponseMessage, data_column_sidecars::DataColumnsByRootIdentifier,
};
use ssz_types::VariableList;
use tokio::{
    sync::mpsc::{self, UnboundedSender},
    task::JoinHandle,
};
use tracing::{info, warn};

/// Result of a sampling attempt for a single slot.
#[derive(Debug, Clone)]
pub struct SamplingResult {
    pub success: bool,
    pub columns_requested: Vec<u64>,
    pub columns_retrieved: Vec<u64>,
    pub columns_missing: Vec<u64>,
}

/// Download task for a column sampling request to a specific peer.
struct ColumnDownloadTask {
    handle: JoinHandle<anyhow::Result<anyhow::Result<Vec<DataColumnSidecar>>>>,
    peer_id: PeerId,
    columns: Vec<u64>,
}

/// The `ColumnSampler` orchestrates peer sampling for Data Availability Sampling (DAS).
///
/// At each slot, it:
/// 1. Computes the sample count based on `allowed_failures` via `get_extended_sample_count`
/// 2. Selects that many column IDs uniformly at random without replacement
/// 3. Skips columns already held via custody
/// 4. For each remaining column, finds candidate peers that custody it
/// 5. Dispatches `DataColumnSidecarsByRoot` requests to selected peers
/// 6. Records peer sampling scores based on response outcomes
/// 7. Aggregates results — sampling succeeds if missing columns <= `allowed_failures`
pub struct ColumnSampler {
    pub network_state: Arc<NetworkState>,
    pub p2p_sender: UnboundedSender<P2PMessage>,
    pub executor: ReamExecutor,
    /// Number of column retrieval failures tolerated before sampling is considered failed.
    /// When 0, uses `SAMPLES_PER_SLOT` as sample count. When > 0, uses
    /// `get_extended_sample_count(allowed_failures)` which increases the number of
    /// columns sampled to maintain the same false positive rate.
    allowed_failures: u64,
}

impl ColumnSampler {
    pub fn new(
        network_state: Arc<NetworkState>,
        p2p_sender: UnboundedSender<P2PMessage>,
        executor: ReamExecutor,
        allowed_failures: u64,
    ) -> Self {
        Self {
            network_state,
            p2p_sender,
            executor,
            allowed_failures,
        }
    }

    /// Perform column sampling for a given block root.
    ///
    /// Selects random columns, finds peers to query, dispatches parallel downloads,
    /// and returns the sampling result.
    pub async fn sample_columns(&self, block_root: B256) -> SamplingResult {
        // Step 1: Compute the number of columns to sample based on allowed_failures
        let sample_count = if self.allowed_failures == 0 {
            SAMPLES_PER_SLOT
        } else {
            get_extended_sample_count(self.allowed_failures)
        };

        // Step 2: Select sample_count column IDs uniformly at random without replacement
        let mut rng = rand::rngs::StdRng::from_os_rng();
        let selected_indices = sample(&mut rng, NUMBER_OF_COLUMNS as usize, sample_count as usize);
        let selected_columns: Vec<u64> = selected_indices.iter().map(|i| i as u64).collect();

        info!(
            "Peer sampling: selected {} columns (allowed_failures={}) for block root {}: {:?}",
            selected_columns.len(),
            self.allowed_failures,
            block_root,
            selected_columns
        );

        // Step 2: Skip columns we already have from custody
        let local_custody_columns = self.network_state.local_custody_columns();
        let columns_to_sample: Vec<u64> = selected_columns
            .iter()
            .filter(|col| !local_custody_columns.contains(col))
            .copied()
            .collect();

        let columns_already_held: Vec<u64> = selected_columns
            .iter()
            .filter(|col| local_custody_columns.contains(col))
            .copied()
            .collect();

        if columns_to_sample.is_empty() {
            info!(
                "All selected columns are already held via custody. Sampling trivially succeeds."
            );
            return SamplingResult {
                success: true,
                columns_requested: selected_columns.clone(),
                columns_retrieved: selected_columns,
                columns_missing: vec![],
            };
        }

        info!(
            "Peer sampling: {} columns to fetch from peers, {} already held via custody",
            columns_to_sample.len(),
            columns_already_held.len()
        );

        // Step 3: For each column, find candidate peers and group by peer
        // Build peer -> columns mapping, spreading columns across peers
        let mut peer_columns: HashMap<PeerId, Vec<u64>> = HashMap::new();
        let mut columns_without_peers = vec![];

        for &column_id in &columns_to_sample {
            let candidates = self.network_state.peers_for_column(column_id);
            if candidates.is_empty() {
                warn!("No peers available for column {column_id}, sampling may fail");
                columns_without_peers.push(column_id);
                continue;
            }

            // Pick a random peer from candidates to distribute load
            let peer_idx = rng.random_range(0..candidates.len());
            let selected_peer = candidates[peer_idx];

            peer_columns
                .entry(selected_peer)
                .or_default()
                .push(column_id);
        }

        // Step 4: Dispatch parallel download tasks
        let mut tasks: Vec<ColumnDownloadTask> = Vec::new();

        for (peer_id, columns) in &peer_columns {
            let handle = Self::download_columns(
                *peer_id,
                self.p2p_sender.clone(),
                self.executor.clone(),
                block_root,
                columns.clone(),
            );
            tasks.push(ColumnDownloadTask {
                handle,
                peer_id: *peer_id,
                columns: columns.clone(),
            });
        }

        info!(
            "Peer sampling: dispatched {} download tasks to {} peers",
            tasks.len(),
            peer_columns.len()
        );

        // Step 5: Collect results
        let mut columns_retrieved = columns_already_held.clone();
        let mut columns_missing: Vec<u64> = columns_without_peers.clone();

        for task in &mut tasks {
            let result = (&mut task.handle).await;
            match result {
                Ok(Ok(Ok(sidecars))) => {
                    let mut all_returned = true;
                    for sidecar in &sidecars {
                        columns_retrieved.push(sidecar.index);
                    }
                    // Check if any requested columns were not returned
                    for &col in &task.columns {
                        if !sidecars.iter().any(|s| s.index == col) {
                            warn!(
                                "Peer {} did not return column {col} (expected via custody)",
                                task.peer_id
                            );
                            columns_missing.push(col);
                            all_returned = false;
                        }
                    }
                    // Score peer based on whether they returned all requested columns
                    if all_returned {
                        self.network_state.record_sampling_success(task.peer_id);
                    } else {
                        self.network_state.record_sampling_failure(task.peer_id);
                    }
                }
                Ok(Ok(Err(err))) => {
                    warn!("Column download from peer {} failed: {err}", task.peer_id);
                    columns_missing.extend(&task.columns);
                    self.network_state.record_sampling_failure(task.peer_id);
                }
                Ok(Err(err)) => {
                    warn!(
                        "Column download task for peer {} panicked: {err}",
                        task.peer_id
                    );
                    columns_missing.extend(&task.columns);
                    self.network_state.record_sampling_failure(task.peer_id);
                }
                Err(err) => {
                    warn!(
                        "Column download task for peer {} join error: {err}",
                        task.peer_id
                    );
                    columns_missing.extend(&task.columns);
                    self.network_state.record_sampling_failure(task.peer_id);
                }
            }
        }

        columns_retrieved.sort();
        columns_retrieved.dedup();
        columns_missing.sort();
        columns_missing.dedup();

        // Sampling succeeds if missing columns are within the allowed failure tolerance
        let success = columns_missing.len() as u64 <= self.allowed_failures;

        if success {
            info!(
                "Peer sampling succeeded for block root {block_root}: {}/{} columns retrieved",
                columns_retrieved.len(),
                selected_columns.len()
            );
        } else {
            warn!(
                "Peer sampling FAILED for block root {block_root}: {}/{} columns retrieved, missing: {:?}",
                columns_retrieved.len(),
                selected_columns.len(),
                columns_missing
            );
        }

        SamplingResult {
            success,
            columns_requested: selected_columns,
            columns_retrieved,
            columns_missing,
        }
    }

    /// Spawn a background task to download data column sidecars from a specific peer.
    ///
    /// Sends a `DataColumnSidecarsByRoot` request for the given block root and column indices,
    /// then collects all `DataColumnSidecar` responses until the stream ends.
    fn download_columns(
        peer_id: PeerId,
        p2p_sender: UnboundedSender<P2PMessage>,
        executor: ReamExecutor,
        block_root: B256,
        columns: Vec<u64>,
    ) -> JoinHandle<anyhow::Result<anyhow::Result<Vec<DataColumnSidecar>>>> {
        executor.spawn(async move {
            let mut column_sidecars = vec![];
            let (callback, mut rx) = mpsc::channel(100);

            let column_list = VariableList::new(columns).expect("Too many columns requested");

            let identifier = DataColumnsByRootIdentifier {
                block_root,
                columns: column_list,
            };

            p2p_sender
                .send(P2PMessage::Request(P2PRequest::DataColumnsByRoot {
                    peer_id,
                    identifiers: vec![identifier],
                    callback,
                }))
                .expect("Failed to send data columns by root request");

            while let Some(response) = rx.recv().await {
                match response {
                    Ok(P2PCallbackResponse::ResponseMessage(message)) => {
                        if let BeaconResponseMessage::DataColumnSidecarsByRoot(column_sidecar) =
                            message.as_ref().clone()
                        {
                            info!(
                                "Received data column sidecar for column index {}",
                                column_sidecar.index,
                            );
                            column_sidecars.push(column_sidecar);
                        }
                    }
                    Ok(P2PCallbackResponse::EndOfStream) => {
                        info!("End of data column sidecar request stream received.");
                        break;
                    }
                    Ok(P2PCallbackResponse::Disconnected) => {
                        anyhow::bail!("Peer disconnected while receiving data column sidecars.");
                    }
                    Ok(P2PCallbackResponse::Timeout) => {
                        anyhow::bail!("Data column sidecar request timed out.");
                    }
                    Err(err) => {
                        info!("Error receiving DataColumnSidecars from request: {err:?}");
                    }
                }
            }

            Ok(column_sidecars)
        })
    }
}
