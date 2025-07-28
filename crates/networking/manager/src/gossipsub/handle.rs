use libp2p::gossipsub::Message;
use ream_beacon_chain::beacon_chain::BeaconChain;
use ream_consensus_beacon::{
    blob_sidecar::BlobIdentifier, execution_engine::rpc_types::get_blobs::BlobAndProofV1,
};
use ream_consensus_misc::constants::genesis_validators_root;
use ream_network_spec::networks::beacon_network_spec;
use ream_p2p::{
    channel::GossipMessage,
    gossipsub::{
        configurations::GossipsubConfig,
        message::GossipsubMessage,
        topics::{GossipTopic, GossipTopicKind},
    },
};
use ream_storage::{cache::CachedDB, tables::Table};
use ream_validator_beacon::blob_sidecars::compute_subnet_for_blob_sidecar;
use ssz::Encode;
use tracing::{error, info, trace, warn};
use tree_hash::TreeHash;

use crate::{
    gossipsub::validate::{
        attester_slashing::validate_attester_slashing,
        beacon_attestation::validate_beacon_attestation,
        beacon_block::validate_gossip_beacon_block, blob_sidecar::validate_blob_sidecar,
        bls_to_execution_change::validate_bls_to_execution_change,
        proposer_slashing::validate_proposer_slashing, result::ValidationResult,
        sync_committee::validate_sync_committee, voluntary_exit::validate_voluntary_exit,
    },
    p2p_sender::P2PSender,
};

pub fn init_gossipsub_config_with_topics() -> GossipsubConfig {
    let mut gossipsub_config = GossipsubConfig::default();

    gossipsub_config.set_topics(vec![
        GossipTopic {
            fork: beacon_network_spec().fork_digest(genesis_validators_root()),
            kind: GossipTopicKind::BeaconBlock,
        },
        GossipTopic {
            fork: beacon_network_spec().fork_digest(genesis_validators_root()),
            kind: GossipTopicKind::AggregateAndProof,
        },
        GossipTopic {
            fork: beacon_network_spec().fork_digest(genesis_validators_root()),
            kind: GossipTopicKind::VoluntaryExit,
        },
        GossipTopic {
            fork: beacon_network_spec().fork_digest(genesis_validators_root()),
            kind: GossipTopicKind::ProposerSlashing,
        },
        GossipTopic {
            fork: beacon_network_spec().fork_digest(genesis_validators_root()),
            kind: GossipTopicKind::AttesterSlashing,
        },
        GossipTopic {
            fork: beacon_network_spec().fork_digest(genesis_validators_root()),
            kind: GossipTopicKind::BeaconAttestation(0),
        },
        GossipTopic {
            fork: beacon_network_spec().fork_digest(genesis_validators_root()),
            kind: GossipTopicKind::SyncCommittee(0),
        },
        GossipTopic {
            fork: beacon_network_spec().fork_digest(genesis_validators_root()),
            kind: GossipTopicKind::SyncCommitteeContributionAndProof,
        },
        GossipTopic {
            fork: beacon_network_spec().fork_digest(genesis_validators_root()),
            kind: GossipTopicKind::BlsToExecutionChange,
        },
        GossipTopic {
            fork: beacon_network_spec().fork_digest(genesis_validators_root()),
            kind: GossipTopicKind::LightClientFinalityUpdate,
        },
        GossipTopic {
            fork: beacon_network_spec().fork_digest(genesis_validators_root()),
            kind: GossipTopicKind::LightClientOptimisticUpdate,
        },
        GossipTopic {
            fork: beacon_network_spec().fork_digest(genesis_validators_root()),
            kind: GossipTopicKind::BlobSidecar(0),
        },
        GossipTopic {
            fork: beacon_network_spec().fork_digest(genesis_validators_root()),
            kind: GossipTopicKind::VoluntaryExit,
        },
    ]);

    gossipsub_config
}

/// Dispatches a gossipsub message to its appropriate handler.
pub async fn handle_gossipsub_message(
    message: Message,
    beacon_chain: &BeaconChain,
    cached_db: &CachedDB,
    p2p_sender: &P2PSender,
) {
    match GossipsubMessage::decode(&message.topic, &message.data) {
        Ok(gossip_message) => match gossip_message {
            GossipsubMessage::BeaconBlock(signed_block) => {
                info!(
                    "Beacon block received over gossipsub: slot: {}, root: {}",
                    signed_block.message.slot,
                    signed_block.message.block_root()
                );

                let validation_result = match validate_gossip_beacon_block(
                    beacon_chain,
                    cached_db,
                    &signed_block,
                )
                .await
                {
                    Ok(result) => result,
                    Err(err) => {
                        warn!("Failed to validate gossipsub beacon block: {err}");
                        return;
                    }
                };

                match validation_result {
                    ValidationResult::Accept => {
                        let signed_block_bytes = signed_block.as_ssz_bytes();
                        if let Err(err) = beacon_chain.process_block(*signed_block).await {
                            error!("Failed to process gossipsub beacon block: {err}");
                        }
                        p2p_sender.send_gossip(GossipMessage {
                            topic: GossipTopic::from_topic_hash(&message.topic)
                                .expect("invalid topic hash"),
                            data: signed_block_bytes,
                        });
                    }
                    ValidationResult::Ignore(reason) => {
                        warn!("Ignoring gossipsub beacon block: {reason}");
                    }
                    ValidationResult::Reject(reason) => {
                        warn!("Rejecting gossipsub beacon block: {reason}");
                    }
                }
            }
            GossipsubMessage::BeaconAttestation((single_attestation, subnet_id)) => {
                info!(
                    "Beacon Attestation received over gossipsub: root: {}",
                    single_attestation.tree_hash_root()
                );

                match validate_beacon_attestation(
                    &single_attestation,
                    beacon_chain,
                    subnet_id,
                    cached_db,
                )
                .await
                {
                    Ok(validation_result) => match validation_result {
                        ValidationResult::Accept => {
                            p2p_sender.send_gossip(GossipMessage {
                                topic: GossipTopic::from_topic_hash(&message.topic)
                                    .expect("invalid topic hash"),
                                data: single_attestation.as_ssz_bytes(),
                            });
                        }
                        ValidationResult::Reject(reason) => {
                            info!("Attestation rejected: {reason}");
                        }
                        ValidationResult::Ignore(reason) => {
                            info!("Attestation ignored: {reason}");
                        }
                    },
                    Err(err) => {
                        error!("Could not validate attestation: {err}");
                    }
                }
            }
            GossipsubMessage::BlsToExecutionChange(signed_bls_to_execution_change) => {
                info!(
                    "BLS to Execution Change received over gossipsub: root: {}",
                    signed_bls_to_execution_change.tree_hash_root()
                );

                match validate_bls_to_execution_change(
                    &signed_bls_to_execution_change,
                    beacon_chain,
                    cached_db,
                )
                .await
                {
                    Ok(validation_result) => match validation_result {
                        ValidationResult::Accept => {
                            p2p_sender.send_gossip(GossipMessage {
                                topic: GossipTopic::from_topic_hash(&message.topic)
                                    .expect("invalid topic hash"),
                                data: signed_bls_to_execution_change.as_ssz_bytes(),
                            });
                        }
                        ValidationResult::Reject(reason) => {
                            info!("BLS to Execution Change rejected: {reason}");
                        }
                        ValidationResult::Ignore(reason) => {
                            info!("BLS to Execution Change ignored: {reason}");
                        }
                    },
                    Err(err) => {
                        error!("Could not validate BLS to Execution Change: {err}");
                    }
                }
            }
            GossipsubMessage::AggregateAndProof(aggregate_and_proof) => {
                info!(
                    "Aggregate And Proof received over gossipsub: root: {}",
                    aggregate_and_proof.tree_hash_root()
                );
            }
            GossipsubMessage::SyncCommittee((sync_committee, subnet_id)) => {
                info!(
                    "Sync Committee received over gossipsub: root: {}",
                    sync_committee.tree_hash_root()
                );

                match validate_sync_committee(&sync_committee, beacon_chain, subnet_id, cached_db)
                    .await
                {
                    Ok(validation_result) => match validation_result {
                        ValidationResult::Accept => {
                            p2p_sender.send_gossip(GossipMessage {
                                topic: GossipTopic::from_topic_hash(&message.topic)
                                    .expect("invalid topic hash"),
                                data: sync_committee.as_ssz_bytes(),
                            });
                        }
                        ValidationResult::Reject(reason) => {
                            info!("Sync committee message rejected: {reason}");
                        }
                        ValidationResult::Ignore(reason) => {
                            info!("Sync committee message ignored: {reason}");
                        }
                    },
                    Err(err) => {
                        error!("Could not validate sync committee message: {err}");
                    }
                }
            }
            GossipsubMessage::SyncCommitteeContributionAndProof(
                _sync_committee_contribution_and_proof,
            ) => {}
            GossipsubMessage::AttesterSlashing(attester_slashing) => {
                info!(
                    "Attester Slashing received over gossipsub: root: {}",
                    attester_slashing.tree_hash_root()
                );

                match validate_attester_slashing(&attester_slashing, beacon_chain, cached_db).await
                {
                    Ok(validation_result) => match validation_result {
                        ValidationResult::Accept => {
                            p2p_sender.send_gossip(GossipMessage {
                                topic: GossipTopic::from_topic_hash(&message.topic)
                                    .expect("invalid topic hash"),
                                data: attester_slashing.as_ssz_bytes(),
                            });
                            if let Err(err) = beacon_chain
                                .process_attester_slashing(*attester_slashing)
                                .await
                            {
                                error!("Failed to process gossipsub attester slashing: {err}");
                            }
                        }
                        ValidationResult::Reject(reason) => {
                            info!("Attester slashing rejected: {reason}");
                        }
                        ValidationResult::Ignore(reason) => {
                            info!("Attester slashing ignored: {reason}");
                        }
                    },
                    Err(err) => {
                        error!("Could not validate attester slashing: {err}");
                    }
                }
            }
            GossipsubMessage::ProposerSlashing(proposer_slashing) => {
                info!(
                    "Proposer Slashing received over gossipsub: root: {}",
                    proposer_slashing.tree_hash_root()
                );

                match validate_proposer_slashing(&proposer_slashing, beacon_chain, cached_db).await
                {
                    Ok(validation_result) => match validation_result {
                        ValidationResult::Accept => {
                            p2p_sender.send_gossip(GossipMessage {
                                topic: GossipTopic::from_topic_hash(&message.topic)
                                    .expect("invalid topic hash"),
                                data: proposer_slashing.as_ssz_bytes(),
                            });
                        }
                        ValidationResult::Reject(reason) => {
                            info!("Proposer slashing rejected: {reason}");
                        }
                        ValidationResult::Ignore(reason) => {
                            info!("Proposer slashing ignored: {reason}");
                        }
                    },
                    Err(err) => {
                        error!("Could not validate proposer slashing: {err}");
                    }
                }
            }
            GossipsubMessage::BlobSidecar(blob_sidecar) => {
                info!(
                    "Blob Sidecar received over gossipsub: root: {}",
                    blob_sidecar.tree_hash_root()
                );
                match validate_blob_sidecar(
                    beacon_chain,
                    &blob_sidecar,
                    compute_subnet_for_blob_sidecar(blob_sidecar.index),
                    cached_db,
                )
                .await
                {
                    Ok(validation_result) => match validation_result {
                        ValidationResult::Accept => {
                            let blob_sidecar_bytes = blob_sidecar.as_ssz_bytes();
                            if let Err(err) = beacon_chain
                                .store
                                .lock()
                                .await
                                .db
                                .blobs_and_proofs_provider()
                                .insert(
                                    BlobIdentifier::new(
                                        blob_sidecar.signed_block_header.message.tree_hash_root(),
                                        blob_sidecar.index,
                                    ),
                                    BlobAndProofV1 {
                                        blob: blob_sidecar.blob,
                                        proof: blob_sidecar.kzg_proof,
                                    },
                                )
                            {
                                error!("Failed to insert blob_sidecar: {err}");
                            }

                            p2p_sender.send_gossip(GossipMessage {
                                topic: GossipTopic::from_topic_hash(&message.topic)
                                    .expect("invalid topic hash"),
                                data: blob_sidecar_bytes,
                            });
                        }
                        ValidationResult::Reject(reason) => {
                            info!("Blob_sidecar rejected: {reason}");
                        }
                        ValidationResult::Ignore(reason) => {
                            info!("Blob_sidecar ignored: {reason}");
                        }
                    },
                    Err(err) => {
                        error!("Could not validate blob_sidecar: {err}");
                    }
                }
            }
            GossipsubMessage::LightClientFinalityUpdate(light_client_finality_update) => {
                info!(
                    "Light Client Finality Update received over gossipsub: root: {}",
                    light_client_finality_update.tree_hash_root()
                );
            }
            GossipsubMessage::LightClientOptimisticUpdate(light_client_optimistic_update) => {
                info!(
                    "Light Client Optimistic Update received over gossipsub: root: {}",
                    light_client_optimistic_update.tree_hash_root()
                );
            }
            GossipsubMessage::VoluntaryExit(voluntary_exit) => {
                info!(
                    "Voluntary Exit received over gossipsub: root: {}",
                    voluntary_exit.tree_hash_root()
                );

                match validate_voluntary_exit(&voluntary_exit, beacon_chain, cached_db).await {
                    Ok(validation_result) => match validation_result {
                        ValidationResult::Accept => {
                            p2p_sender.send_gossip(GossipMessage {
                                topic: GossipTopic::from_topic_hash(&message.topic)
                                    .expect("invalid topic hash"),
                                data: voluntary_exit.as_ssz_bytes(),
                            });
                        }
                        ValidationResult::Reject(reason) => {
                            info!("voluntary_exit rejected: {reason}");
                        }
                        ValidationResult::Ignore(reason) => {
                            info!("voluntary_exit ignored: {reason}");
                        }
                    },
                    Err(err) => {
                        error!("Could not validate voluntary_exit: {err}");
                    }
                }
            }
        },
        Err(err) => {
            trace!("Failed to decode gossip message: {err:?}");
        }
    };
}
