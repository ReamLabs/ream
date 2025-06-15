mod block_cache;
mod peer_manager;
mod peer_range_downloader;

use std::{
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::Duration,
};

use alloy_primitives::B256;
use anyhow::{anyhow, bail};
use block_cache::{BlockAndBlobBundle, BlockCache, DataToFetch, HUNDRED_MEGA_BYTES};
use futures::task::noop_waker;
use libp2p::PeerId;
use peer_manager::PeerManager;
use peer_range_downloader::{PeerBlobIdentifierDownloader, PeerRootsDownloader};
use ream_beacon_chain::beacon_chain::BeaconChain;
use ream_consensus::{
    blob_sidecar::{BlobIdentifier, BlobSidecar},
    electra::beacon_block::SignedBeaconBlock,
};
use ream_executor::ReamExecutor;
use ream_p2p::{
    channel::P2PMessage, network_state::NetworkState, req_resp::MAX_CONCURRENT_REQUESTS,
};
use ream_storage::tables::Table;
use tokio::{sync::mpsc::UnboundedSender, task::JoinHandle, time::sleep};
use tracing::{info, warn};

use crate::block_range::peer_range_downloader::{PeerRangeDownloader, Range};

const MAX_BLOBS_PER_REQUEST: usize = 6;
const MAX_BLOCKS_PER_REQUEST: u64 = 30;
const SLEEP_DURATION: Duration = Duration::from_secs(5);

pub struct BlockRangeSyncer {
    pub beacon_chain: Arc<BeaconChain>,
    pub peer_manager: PeerManager,
    pub p2p_sender: UnboundedSender<P2PMessage>,
    pub executor: ReamExecutor,
}

impl BlockRangeSyncer {
    pub fn new(
        beacon_chain: Arc<BeaconChain>,
        p2p_sender: UnboundedSender<P2PMessage>,
        network_state: Arc<NetworkState>,
        executor: ReamExecutor,
    ) -> Self {
        Self {
            beacon_chain,
            p2p_sender,
            peer_manager: PeerManager::new(network_state),
            executor,
        }
    }

    pub async fn is_synced_to_finalized_slot(&self) -> bool {
        let finalized_slot = self.peer_manager.finalized_slot();
        let latest_synced_slot = self
            .beacon_chain
            .store
            .lock()
            .await
            .db
            .slot_index_provider()
            .get_highest_slot()
            .unwrap_or_default()
            .unwrap_or(0);

        finalized_slot <= Some(latest_synced_slot)
    }

    pub fn start(mut self) -> JoinHandle<anyhow::Result<anyhow::Result<BlockRangeSyncer>>> {
        let executor = self.executor.clone();
        executor.spawn(async move {
            let Some(latest_synced_root) = self
                .beacon_chain
                .store
                .lock()
                .await
                .db
                .slot_index_provider()
                .get_highest_root()
                .map_err(|err| anyhow!("Failed to get highest root: {err}"))?
            else {
                bail!("No synced root found in the database");
            };

            let Some(latest_synced_slot) = self
                .beacon_chain
                .store
                .lock()
                .await
                .db
                .slot_index_provider()
                .get_highest_slot()
                .map_err(|err| anyhow!("Failed to get highest slot: {err}"))?
            else {
                bail!("No synced slot found in the database");
            };

            // phase 1: download majority of blocks from ranges
            let mut block_cache =
                BlockCache::new(HUNDRED_MEGA_BYTES, latest_synced_root, latest_synced_slot);
            let mut task_handles = vec![];
            loop {
                poll_ready_tasks(&mut task_handles, &mut block_cache, &mut self.peer_manager)?;

                let finalized_slot = match self.peer_manager.finalized_slot() {
                    Some(finalized_slot) => finalized_slot,
                    None => {
                        warn!("No peers available to determine finalized slot, retrying...");
                        sleep(SLEEP_DURATION).await;
                        self.peer_manager.update_peer_set();
                        continue;
                    }
                };

                let data_to_fetch = block_cache.data_to_fetch(finalized_slot);
                info!(
                    "Forward sync status: Downloaded Blocks {}, Downloaded Blobs {}/{}, Stage {data_to_fetch}",
                    block_cache.block_count(),
                    block_cache.downloaded_blob_count(),
                    block_cache.blob_count(),
                );

                match data_to_fetch {
                    DataToFetch::BlockRange(range) => {
                        let Some(peer) = self.peer_manager.fetch_idle_peer() else {
                            self.peer_manager.update_peer_set();
                            info!("No idle peers available for block range sync.");
                            sleep(SLEEP_DURATION).await;
                            continue;
                        };

                        task_handles.push(DownloadTask::new_block_range(
                            PeerRangeDownloader::start(
                                peer.peer_id,
                                self.p2p_sender.clone(),
                                self.executor.clone(),
                                range,
                            ),
                            range,
                            peer.peer_id,
                        ));
                    }
                    DataToFetch::MissingBlockRoots(block_roots) => {
                        for block_roots_chunk in block_roots.chunks(MAX_CONCURRENT_REQUESTS) {
                            let Some(peer) = self.peer_manager.fetch_idle_peer() else {
                                self.peer_manager.update_peer_set();
                                info!("No idle peers available for block roots sync.");
                                sleep(SLEEP_DURATION).await;
                                break;
                            };
                            block_cache.extend_block_roots_in_progress(block_roots_chunk);

                            task_handles.push(DownloadTask::new_block_roots(
                                PeerRootsDownloader::start(
                                    peer.peer_id,
                                    self.p2p_sender.clone(),
                                    self.executor.clone(),
                                    block_roots_chunk.to_vec(),
                                ),
                                block_roots_chunk.to_vec(),
                                peer.peer_id,
                            ));
                        }
                    }
                    DataToFetch::MissingBlobIdentifiers(blob_identifiers) => {
                        for blob_identifiers_chunk in blob_identifiers.chunks(MAX_BLOBS_PER_REQUEST) {
                            let Some(peer) = self.peer_manager.fetch_idle_peer() else {
                                self.peer_manager.update_peer_set();
                                info!("No idle peers available for blob sync.");
                                sleep(SLEEP_DURATION).await;
                                break;
                            };

                            block_cache.extend_blob_identifiers_in_progress(blob_identifiers_chunk);

                            task_handles.push(DownloadTask::new_blob_identifiers(
                                PeerBlobIdentifierDownloader::start(
                                    peer.peer_id,
                                    self.p2p_sender.clone(),
                                    self.executor.clone(),
                                    blob_identifiers_chunk.to_vec(),
                                ),
                                blob_identifiers_chunk.to_vec(),
                                peer.peer_id,
                            ));
                        }
                    }
                    DataToFetch::DownloadsInProgress => {
                        sleep(Duration::from_secs(10)).await;
                    }
                    DataToFetch::Finished => break,
                }
            }

            info!("Block range sync completed a segment successfully with {} blocks and {} blobs.",
                block_cache.block_count(),
                block_cache.downloaded_blob_count(),
            );

            // execute all the blocks downloaded
            for BlockAndBlobBundle { block, blobs } in block_cache.get_blocks_and_blobs()?  {
                for (blob_identifier, blob_sidecar) in blobs {
                    if let Err(err) = self
                        .beacon_chain
                        .store
                        .lock()
                        .await
                        .db
                        .blobs_and_proofs_provider()
                        .insert(blob_identifier, blob_sidecar.into())
                    {
                        warn!("Failed to insert blob into database: {err}");
                    }
                }

                self.beacon_chain.process_block(block).await?;
            }

            Ok(self)
        })
    }
}

pub enum DownloadTask {
    BlockRange {
        handle: JoinHandle<anyhow::Result<Vec<SignedBeaconBlock>>>,
        range: Range,
        peer_id: PeerId,
    },
    BlockRoots {
        handle: JoinHandle<anyhow::Result<Vec<SignedBeaconBlock>>>,
        roots: Vec<B256>,
        peer_id: PeerId,
    },
    BlobIdentifiers {
        handle: JoinHandle<anyhow::Result<Vec<BlobSidecar>>>,
        blob_identifiers: Vec<BlobIdentifier>,
        peer_id: PeerId,
    },
}

impl DownloadTask {
    pub fn new_block_range(
        handle: JoinHandle<anyhow::Result<Vec<SignedBeaconBlock>>>,
        range: Range,
        peer_id: PeerId,
    ) -> Self {
        DownloadTask::BlockRange {
            handle,
            range,
            peer_id,
        }
    }

    pub fn new_block_roots(
        handle: JoinHandle<anyhow::Result<Vec<SignedBeaconBlock>>>,
        roots: Vec<B256>,
        peer_id: PeerId,
    ) -> Self {
        DownloadTask::BlockRoots {
            handle,
            roots,
            peer_id,
        }
    }

    pub fn new_blob_identifiers(
        handle: JoinHandle<anyhow::Result<Vec<BlobSidecar>>>,
        blob_identifiers: Vec<BlobIdentifier>,
        peer_id: PeerId,
    ) -> Self {
        DownloadTask::BlobIdentifiers {
            handle,
            blob_identifiers,
            peer_id,
        }
    }
}

fn poll_ready_tasks(
    tasks: &mut Vec<DownloadTask>,
    block_cache: &mut BlockCache,
    peer_manager: &mut PeerManager,
) -> anyhow::Result<()> {
    let waker = noop_waker();
    let mut context = Context::from_waker(&waker);
    let mut indexes_to_remove = vec![];

    for index in (0..tasks.len()).rev() {
        let Some(task) = tasks.get_mut(index) else {
            bail!("Task handle not found at index {index}");
        };

        match task {
            DownloadTask::BlockRange {
                handle,
                range,
                peer_id,
            } => {
                let pinned = Pin::new(handle);

                match pinned.poll(&mut context) {
                    Poll::Ready(Ok(blocks_result)) => {
                        indexes_to_remove.push(index);
                        peer_manager.mark_peer_as_idle(peer_id);
                        let blocks = match blocks_result {
                            Ok(blocks) => blocks,
                            Err(err) => {
                                warn!("Failed to fetch blocks from peer: {err:?}");
                                block_cache.push_retry_range(*range);
                                continue;
                            }
                        };

                        if blocks.is_empty() {
                            warn!("Received empty block range from peer: {peer_id}");
                            block_cache.push_retry_range(*range);
                            peer_manager.ban_peer(peer_id);
                            continue;
                        }

                        if let Err(err) = block_cache.add_blocks(blocks) {
                            warn!("Failed to add downloaded blocks to cache: {err:?}");
                            block_cache.push_retry_range(*range);
                        }
                    }
                    Poll::Ready(Err(err)) => {
                        warn!("Forward fill task failed: {err}");
                        indexes_to_remove.push(index);
                    }
                    Poll::Pending => {}
                }
            }
            DownloadTask::BlockRoots {
                handle,
                roots,
                peer_id,
            } => {
                let pinned = Pin::new(handle);

                match pinned.poll(&mut context) {
                    Poll::Ready(Ok(blocks_result)) => {
                        indexes_to_remove.push(index);
                        block_cache.remove_block_roots_in_progress(roots);
                        peer_manager.mark_peer_as_idle(peer_id);
                        let blocks = match blocks_result {
                            Ok(blocks) => blocks,
                            Err(err) => {
                                warn!("Failed to fetch blocks from peer: {err:?}");
                                continue;
                            }
                        };

                        if blocks.is_empty() {
                            warn!("Received empty block roots from peer: {peer_id}");
                            peer_manager.ban_peer(peer_id);
                            continue;
                        }

                        if let Err(err) = block_cache.add_blocks(blocks) {
                            warn!("Failed to add downloaded blocks to cache: {err:?}");
                        }
                    }
                    Poll::Ready(Err(err)) => {
                        warn!("Forward fill task failed: {err}");
                        indexes_to_remove.push(index);
                    }
                    Poll::Pending => {}
                }
            }
            DownloadTask::BlobIdentifiers {
                handle,
                blob_identifiers,
                peer_id,
            } => {
                let pinned = Pin::new(handle);

                match pinned.poll(&mut context) {
                    Poll::Ready(Ok(blob_sidecars_result)) => {
                        indexes_to_remove.push(index);
                        block_cache.remove_blob_identifiers_in_progress(blob_identifiers);
                        peer_manager.mark_peer_as_idle(peer_id);
                        let blob_sidecars = match blob_sidecars_result {
                            Ok(blob_sidecars) => blob_sidecars,
                            Err(err) => {
                                warn!("Failed to fetch blobs from peer: {err:?}");
                                continue;
                            }
                        };

                        if blob_sidecars.is_empty() {
                            warn!("Received empty blob identifiers from peer: {peer_id}");
                            peer_manager.ban_peer(peer_id);
                            continue;
                        }

                        if let Err(err) = block_cache.add_blobs(blob_sidecars) {
                            warn!("Failed to add downloaded blobs to cache: {err:?}");
                        }
                    }
                    Poll::Ready(Err(err)) => {
                        warn!("Forward fill task failed: {err}");
                        indexes_to_remove.push(index);
                    }
                    Poll::Pending => {}
                }
            }
        }
    }

    for index in indexes_to_remove {
        tasks.remove(index);
    }

    Ok(())
}
