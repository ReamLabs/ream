use std::{sync::Arc, time::Instant};

use alloy_primitives::B256;
use anyhow::anyhow;
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
        let network_finalized_slot = self
            .network_state
            .common_finalized_checkpoint()
            .map(|checkpoint| checkpoint.slot)
            .unwrap_or(0);
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
                        return Ok(ForwardSyncResults::ChainIncomplete {
                            prevous_queue: self.job_queue.clone(),
                            checkpoint_for_new_queue: Checkpoint {
                                root: next_root,
                                slot: last_block
                                    .as_ref()
                                    .map(|last_block| {
                                        last_block.message.block.slot.saturating_sub(1)
                                    })
                                    .unwrap_or(self.job_queue.starting_slot),
                            },
                        });
                    }
                },
            };
            if current_block.message.block.tree_hash_root() != next_root {
                let bad_slot = current_block.message.block.slot;
                return Ok(ForwardSyncResults::RootMismatch {
                    previous_queue: self.job_queue.clone(),
                    checkpoint_for_new_queue: (bad_slot > network_finalized_slot).then_some(
                        Checkpoint {
                            root: next_root,
                            slot: bad_slot,
                        },
                    ),
                    bad_root: next_root,
                    bad_slot,
                    actual_root: current_block.message.block.tree_hash_root(),
                    network_finalized_slot,
                });
            }
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
                        return Ok(ForwardSyncResults::ChainIncomplete {
                            prevous_queue: self.job_queue.clone(),
                            checkpoint_for_new_queue: Checkpoint {
                                root: next_root,
                                slot: last_block
                                    .as_ref()
                                    .map(|last_block| last_block.block.slot.saturating_sub(1))
                                    .unwrap_or(self.job_queue.starting_slot),
                            },
                        });
                    }
                },
            };
            if current_block.block.tree_hash_root() != next_root {
                let bad_slot = current_block.block.slot;
                return Ok(ForwardSyncResults::RootMismatch {
                    previous_queue: self.job_queue.clone(),
                    checkpoint_for_new_queue: (bad_slot > network_finalized_slot).then_some(
                        Checkpoint {
                            root: next_root,
                            slot: bad_slot,
                        },
                    ),
                    bad_root: next_root,
                    bad_slot,
                    actual_root: current_block.block.tree_hash_root(),
                    network_finalized_slot,
                });
            }
            chained_roots.push(next_root);
            next_root = current_block.block.parent_root;
            last_block = Some(current_block.clone());
        }

        chained_roots.reverse();
        let mut blocks_synced = 0usize;
        let mut imported_start_slot = None;
        let mut imported_end_slot = None;

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
                + (block.block.slot * lean_network_spec().seconds_per_slot);
            #[cfg(feature = "devnet3")]
            let block_slot = block.message.block.slot;
            #[cfg(feature = "devnet4")]
            let block_slot = block.block.slot;
            store_writer.on_tick(time, false, true).await?;
            store_writer.on_block(&block, true).await?;
            blocks_synced += 1;
            if imported_start_slot.is_none() {
                imported_start_slot = Some(block_slot);
            }
            imported_end_slot = Some(block_slot);
            // Remove blocks that have been applied to canonical storage to prevent unbounded growth
            // of the pending-blocks table.
            let _ = pending_blocks_provider.remove(root)?;
        }

        Ok(ForwardSyncResults::Completed {
            starting_root: head,
            ending_root: self.job_queue.starting_root,
            imported_start_slot,
            imported_end_slot,
            blocks_synced,
            processing_time_seconds: timer.elapsed().as_secs_f64(),
        })
    }
}

#[derive(Debug)]
pub enum ForwardSyncResults {
    Completed {
        starting_root: B256,
        ending_root: B256,
        imported_start_slot: Option<u64>,
        imported_end_slot: Option<u64>,
        blocks_synced: usize,
        processing_time_seconds: f64,
    },
    ChainIncomplete {
        prevous_queue: JobQueue,
        checkpoint_for_new_queue: Checkpoint,
    },
    RootMismatch {
        previous_queue: JobQueue,
        checkpoint_for_new_queue: Option<Checkpoint>,
        bad_root: B256,
        bad_slot: u64,
        actual_root: B256,
        network_finalized_slot: u64,
    },
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use libp2p_identity::PeerId;
    #[cfg(feature = "devnet4")]
    use ream_consensus_lean::block::{BlockSignatures, SignedBlock};
    #[cfg(feature = "devnet3")]
    use ream_consensus_lean::{
        attestation::AggregatedAttestations,
        block::{BlockSignatures, BlockWithAttestation, SignedBlockWithAttestation},
    };
    use ream_peer::{ConnectionState, Direction};
    use ream_post_quantum_crypto::leansig::signature::Signature;
    use ream_sync::rwlock::Writer;
    use ream_test_utils::store::sample_store;

    use super::*;

    async fn root_mismatch_result(network_finalized_slot: u64) -> ForwardSyncResults {
        let mut store = sample_store(10).await;
        #[cfg(feature = "devnet3")]
        let attestation = store.produce_attestation_data(1).await.unwrap();
        let block = store.produce_block_with_signatures(1, 1).await.unwrap();
        #[cfg(feature = "devnet3")]
        let pending_block = SignedBlockWithAttestation {
            message: BlockWithAttestation {
                block: block.block,
                proposer_attestation: AggregatedAttestations {
                    validator_id: 1,
                    data: attestation,
                },
            },
            signature: BlockSignatures {
                attestation_signatures: block.signatures,
                proposer_signature: Signature::mock(),
            },
        };
        #[cfg(feature = "devnet4")]
        let pending_block = SignedBlock {
            block: block.block,
            signature: BlockSignatures {
                attestation_signatures: block.signatures,
                proposer_signature: Signature::mock(),
            },
        };
        let bad_root = B256::repeat_byte(0xef);
        store
            .store
            .lock()
            .await
            .pending_blocks_provider()
            .insert(bad_root, pending_block)
            .unwrap();

        let (writer, _reader) = Writer::new(store);
        let network_state = writer.read().await.network_state.clone();
        let peer_id = PeerId::random();
        network_state.upsert_peer(
            peer_id,
            None,
            ConnectionState::Connected,
            Direction::Outbound,
        );
        network_state.update_peer_checkpoints(
            peer_id,
            Checkpoint {
                root: B256::repeat_byte(0x11),
                slot: 1,
            },
            Checkpoint {
                root: B256::repeat_byte(0x22),
                slot: network_finalized_slot,
            },
        );

        let mut queue = JobQueue::new(bad_root, 1, 1);
        queue.is_complete = true;
        let mut syncer = ForwardBackgroundSyncer::new(Arc::new(writer), network_state, queue);
        syncer.start().await.unwrap()
    }

    #[tokio::test]
    async fn test_root_mismatch_requeues_before_network_finalized() {
        let result = root_mismatch_result(0).await;

        match result {
            ForwardSyncResults::RootMismatch {
                checkpoint_for_new_queue,
                bad_root,
                bad_slot,
                network_finalized_slot,
                ..
            } => {
                assert_eq!(bad_root, B256::repeat_byte(0xef));
                assert_eq!(bad_slot, 1);
                assert_eq!(network_finalized_slot, 0);
                assert_eq!(
                    checkpoint_for_new_queue,
                    Some(Checkpoint {
                        root: B256::repeat_byte(0xef),
                        slot: 1,
                    })
                );
            }
            other => panic!("expected root mismatch result, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_root_mismatch_drops_after_network_finalized() {
        let result = root_mismatch_result(1).await;

        match result {
            ForwardSyncResults::RootMismatch {
                checkpoint_for_new_queue,
                bad_root,
                bad_slot,
                network_finalized_slot,
                ..
            } => {
                assert_eq!(bad_root, B256::repeat_byte(0xef));
                assert_eq!(bad_slot, 1);
                assert_eq!(network_finalized_slot, 1);
                assert_eq!(checkpoint_for_new_queue, None);
            }
            other => panic!("expected root mismatch result, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_missing_starting_root_requeues_from_same_checkpoint() {
        let store = sample_store(10).await;
        let (writer, _reader) = Writer::new(store);
        let network_state = writer.read().await.network_state.clone();
        let mut queue = JobQueue::new(B256::repeat_byte(0xaa), 7, 7);
        queue.is_complete = true;

        let mut syncer = ForwardBackgroundSyncer::new(Arc::new(writer), network_state, queue);
        let result = syncer.start().await.unwrap();

        match result {
            ForwardSyncResults::ChainIncomplete {
                checkpoint_for_new_queue,
                prevous_queue,
            } => {
                assert_eq!(prevous_queue.starting_root, B256::repeat_byte(0xaa));
                assert_eq!(checkpoint_for_new_queue.root, B256::repeat_byte(0xaa));
                assert_eq!(checkpoint_for_new_queue.slot, 7);
            }
            other => panic!("expected chain incomplete result, got {other:?}"),
        }
    }
}
