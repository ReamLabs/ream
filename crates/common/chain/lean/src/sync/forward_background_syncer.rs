use std::{sync::Arc, time::Instant};

use alloy_primitives::B256;
use anyhow::{anyhow, ensure};
#[cfg(feature = "devnet4")]
use ream_consensus_lean::{block::SignedBlock, checkpoint::Checkpoint};
#[cfg(feature = "devnet3")]
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
        let (head, pending_blocks_provider, block_provider) = {
            let fork_choice = self.store.read().await;
            let store = fork_choice.store.lock().await;
            (
                store.head_provider().get()?,
                store.pending_blocks_provider(),
                store.block_provider(),
            )
        };
        let mut next_root = self.job_queue.starting_root;
        #[cfg(feature = "devnet3")]
        let mut last_block: Option<SignedBlockWithAttestation> = None;
        #[cfg(feature = "devnet4")]
        let mut last_block: Option<SignedBlock> = None;
        let mut chained_roots = vec![];
        #[cfg(feature = "devnet3")]
        while next_root != head && next_root != B256::ZERO {
            let current_block = match pending_blocks_provider.get(next_root)? {
                Some(block) => block,
                None => match block_provider.get(next_root)? {
                    Some(block) => block,
                    None => {
                        let last_block = last_block.ok_or_else(|| {
                            anyhow!(
                                "Failed to find block with root {next_root:?} in pending blocks"
                            )
                        })?;
                        return Ok(ForwardSyncResults::ChainIncomplete {
                            prevous_queue: self.job_queue.clone(),
                            checkpoint_for_new_queue: Checkpoint {
                                root: next_root,
                                slot: last_block.message.block.slot.saturating_sub(1),
                            },
                        });
                    }
                },
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

        #[cfg(feature = "devnet4")]
        while next_root != head && next_root != B256::ZERO {
            let current_block = match pending_blocks_provider.get(next_root)? {
                Some(block) => block,
                None => match block_provider.get(next_root)? {
                    Some(block) => block,
                    None => {
                        let last_block = last_block.ok_or_else(|| {
                            anyhow!(
                                "Failed to find block with root {next_root:?} in pending blocks"
                            )
                        })?;
                        return Ok(ForwardSyncResults::ChainIncomplete {
                            prevous_queue: self.job_queue.clone(),
                            checkpoint_for_new_queue: Checkpoint {
                                root: next_root,
                                slot: last_block.message.slot.saturating_sub(1),
                            },
                        });
                    }
                },
            };
            ensure!(
                current_block.message.tree_hash_root() == next_root,
                "Block root mismatch: expected {next_root:?}, got {:?}",
                current_block.message.tree_hash_root()
            );
            chained_roots.push(next_root);
            next_root = current_block.message.parent_root;
            last_block = Some(current_block.clone());
        }

        chained_roots.reverse();
        let mut blocks_synced = 0usize;

        let mut store_writer = self.store.write().await;
        for root in chained_roots {
            if block_provider.get(root)?.is_some() {
                let _ = pending_blocks_provider.remove(root)?;
                continue;
            }

            let block = pending_blocks_provider.get(root)?.ok_or_else(|| {
                anyhow!(
                    "Failed to find block with root {root:?} in pending blocks during insertion"
                )
            })?;
            #[cfg(feature = "devnet3")]
            let time = lean_network_spec().genesis_time
                + (block.message.block.slot * lean_network_spec().seconds_per_slot);
            #[cfg(feature = "devnet4")]
            let time = lean_network_spec().genesis_time
                + (block.message.slot * lean_network_spec().seconds_per_slot);
            store_writer.on_tick(time, false, true).await?;
            store_writer.on_block(&block, true).await?;
            blocks_synced += 1;
            // Remove blocks that have been applied to canonical storage to prevent unbounded growth
            // of the pending-blocks table.
            let _ = pending_blocks_provider.remove(root)?;
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
