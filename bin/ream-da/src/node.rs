use std::{sync::Arc, time::Duration};

use futures::StreamExt;
use ream_da_beacon::{
    DaConsensusClient,
    iface::{ConsensusClient, ConsensusEvent},
};
use ream_da_errors::DaResult;
use ream_da_networking::DaNetworkService;
use ream_da_storage::DaStore;
use ream_p2p::network::beacon::network_state::NetworkState;
use tokio::time::sleep;
use tracing::{error, info, warn};

// MIN_EPOCHS_FOR_DATA_COLUMN_SIDECARS_REQUESTS = 4096
// SLOTS_PER_EPOCH = 32
const RETENTION_SLOTS: u64 = 4096 * 32;
const RECONNECT_BASE_SECS: u64 = 2;
const RECONNECT_MAX_SECS: u64 = 30;

pub struct DaNode {
    store: Arc<DaStore>,
    consensus: DaConsensusClient,
    network: DaNetworkService,
}

impl DaNode {
    pub fn new(
        store: Arc<DaStore>,
        consensus: DaConsensusClient,
        network: DaNetworkService,
    ) -> Self {
        Self {
            store,
            consensus,
            network,
        }
    }

    pub async fn run(self) -> DaResult<()> {
        let DaNode {
            store,
            consensus,
            network,
        } = self;

        let network_state = network.network_state();
        let consensus_handle = tokio::spawn(async move {
            run_consensus_loop(consensus, store, network_state).await;
        });

        let network_handle = tokio::spawn(async move {
            network.start().await;
        });

        tokio::select! {
            result = consensus_handle => {
                error!("Consensus loop exited unexpectedly: {result:?}");
            }
            result = network_handle => {
                error!("Network service exited unexpectedly: {result:?}");
            }
        }

        Ok(())
    }
}

async fn run_consensus_loop(
    consensus: DaConsensusClient,
    store: Arc<DaStore>,
    network_state: Arc<NetworkState>,
) {
    let mut backoff = RECONNECT_BASE_SECS;
    loop {
        match consensus.try_events() {
            Err(e) => {
                warn!(
                    error = %e,
                    retry_in = backoff,
                    "Failed to open beacon SSE stream, retrying"
                );
                sleep(Duration::from_secs(backoff)).await;
                backoff = (backoff * 2).min(RECONNECT_MAX_SECS);
                continue;
            }
            Ok(mut events) => {
                backoff = RECONNECT_BASE_SECS;
                info!("Beacon SSE stream connected");

                while let Some(event) = events.next().await {
                    match event {
                        Ok(ConsensusEvent::Head(head)) => {
                            if let Err(e) = store.record_slot(head.slot, head.block_root) {
                                error!(slot = head.slot, "Failed to record slot: {e}");
                            } else {
                                let mut status = network_state.status.write();
                                status.head_slot = head.slot;
                                status.head_root = head.block_root;
                                info!(slot = head.slot, block_root = %head.block_root, "New head");
                            }
                        }

                        Ok(ConsensusEvent::Reorg(reorg)) => {
                            if let Err(e) = store.record_slot(reorg.slot, reorg.new_head_block) {
                                error!(slot = reorg.slot, "Failed to update slot after reorg: {e}");
                            }
                            warn!(
                                slot = reorg.slot,
                                depth = reorg.depth,
                                old = %reorg.old_head_block,
                                new = %reorg.new_head_block,
                                "Chain reorg — slot index updated"
                            );
                        }

                        Ok(ConsensusEvent::Finalized(epoch)) => {
                            let finalized_slot = epoch * 32;
                            let min_slot = finalized_slot.saturating_sub(RETENTION_SLOTS);

                            match store.prune_before_slot(min_slot).await {
                                Ok(n) if n > 0 => {
                                    info!(pruned = n, min_slot, epoch, "Pruned stale columns");
                                }
                                Err(e) => error!("Pruning failed at epoch {epoch}: {e}"),
                                _ => {}
                            }

                            let mut status = network_state.status.write();
                            status.finalized_epoch = epoch;
                        }

                        Err(e) => {
                            warn!(error = %e, "Consensus event error");
                        }
                    }
                }

                warn!(retry_in = backoff, "Beacon SSE stream ended, reconnecting");
                sleep(Duration::from_secs(backoff)).await;
                backoff = (backoff * 2).min(RECONNECT_MAX_SECS);
            }
        }
    }
}
