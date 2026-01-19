use std::{sync::Arc, time::Instant};

use alloy_primitives::B256;
use anyhow::{anyhow, ensure};
use ream_consensus_lean::{block::SignedBlockWithAttestation, checkpoint::Checkpoint};
use ream_fork_choice_lean::store::LeanStoreWriter;
use ream_network_spec::networks::lean_network_spec;
use ream_network_state_lean::NetworkState;
use ream_storage::tables::{field::REDBField, table::REDBTable};
use tree_hash::TreeHash;

use crate::sync::job::queue::JobQueue;

pub struct ForwardBackgroundSyncer {
    pub store: Arc<LeanStoreWriter>,
    pub network_state: Arc<NetworkState>,
    pub job_queue: JobQueue,
}

impl ForwardBackgroundSyncer {
    pub fn new(
        store: Arc<LeanStoreWriter>,
        network_state: Arc<NetworkState>,
        job_queue: JobQueue,
    ) -> Self {
        ForwardBackgroundSyncer {
            store,
            network_state,
            job_queue,
        }
    }

    pub async fn start(&mut self) -> anyhow::Result<ForwardSyncResults> {
        let timer = Instant::now();
        let (head, pending_blocks_provider) = {
            let fork_choice = self.store.read().await;
            let store = fork_choice.store.lock().await;
            (
                store.head_provider().get()?,
                store.pending_blocks_provider(),
            )
        };
        let mut next_root = self.job_queue.starting_root;
        let mut last_block: Option<SignedBlockWithAttestation> = None;
        let mut chained_roots = vec![];
        while next_root != head {
            let current_block = match pending_blocks_provider.get(next_root)? {
                Some(block) => block,
                None => {
                    let last_block = last_block.ok_or_else(|| {
                        anyhow!("Failed to find block with root {next_root:?} in pending blocks")
                    })?;
                    return Ok(ForwardSyncResults::ChainIncomplete {
                        prevous_queue: self.job_queue.clone(),
                        checkpoint_for_new_queue: Checkpoint {
                            root: last_block.message.block.tree_hash_root(),
                            slot: last_block.message.block.slot,
                        },
                    });
                }
            };
            ensure!(
                current_block.message.block.tree_hash_root() == next_root,
                "Block root mismatch: expected {next_root:?}, got {:?}",
                current_block.message.block.tree_hash_root()
            );
            chained_roots.push(next_root);
            next_root = current_block.message.block.parent_root;
            last_block = Some(current_block.clone());
        }

        chained_roots.reverse();
        let blocks_synced = chained_roots.len();

        for root in chained_roots {
            let block = pending_blocks_provider.get(root)?.ok_or_else(|| {
                anyhow!(
                    "Failed to find block with root {root:?} in pending blocks during insertion"
                )
            })?;
            let time = lean_network_spec().genesis_time
                + (block.message.block.slot * lean_network_spec().seconds_per_slot);
            self.store.write().await.on_tick(time, false).await?;
            self.store.write().await.on_block(&block, true).await?;
        }

        Ok(ForwardSyncResults::Completed {
            starting_root: head,
            ending_root: self.job_queue.starting_root,
            blocks_synced,
            processing_time_seconds: timer.elapsed().as_secs_f64(),
        })
    }
}

pub enum ForwardSyncResults {
    Completed {
        starting_root: B256,
        ending_root: B256,
        blocks_synced: usize,
        processing_time_seconds: f64,
    },
    ChainIncomplete {
        prevous_queue: JobQueue,
        checkpoint_for_new_queue: Checkpoint,
    },
}
