use std::time::{SystemTime, UNIX_EPOCH};

use libp2p::{
    PeerId,
    gossipsub::{Message, MessageAcceptance, MessageId},
};
use ream_chain_beacon::beacon_chain::BeaconChain;
use ream_consensus_beacon::{
    blob_sidecar::BlobIdentifier,
    data_column_sidecar::{ColumnIdentifier, DATA_COLUMN_SIDECAR_SUBNET_COUNT},
    single_attestation::SingleAttestation,
};
use ream_consensus_misc::constants::beacon::{
    FULU_FORK_EPOCH, MIN_ATTESTATION_INCLUSION_DELAY, genesis_validators_root,
};
use ream_execution_rpc_types::get_blobs::BlobAndProofV1;
use ream_network_spec::networks::beacon_network_spec;
use ream_p2p::gossipsub::beacon::{
    configurations::GossipsubConfig,
    message::GossipsubMessage,
    topics::{GossipTopic, GossipTopicKind},
};
use ream_storage::{
    cache::BeaconCacheDB,
    tables::table::{CustomTable, REDBTable},
};
use ream_validator_beacon::{
    attestation::single_attestation_to_attestation, blob_sidecars::compute_subnet_for_blob_sidecar,
    constants::SYNC_COMMITTEE_SUBNET_COUNT,
};
use tracing::{error, info, trace, warn};
use tree_hash::TreeHash;

use crate::{
    gossipsub::validate::{
        aggregate_and_proof::validate_aggregate_and_proof,
        attester_slashing::validate_attester_slashing,
        beacon_attestation::validate_beacon_attestation,
        beacon_block::validate_gossip_beacon_block, blob_sidecar::validate_blob_sidecar,
        bls_to_execution_change::validate_bls_to_execution_change,
        data_column_sidecar::validate_data_column_sidecar_full,
        light_client_finality_update::validate_light_client_finality_update,
        light_client_optimistic_update::validate_light_client_optimistic_update,
        proposer_slashing::validate_proposer_slashing, result::ValidationResult,
        sync_committee::validate_sync_committee,
        sync_committee_contribution_and_proof::validate_sync_committee_contribution_and_proof,
        voluntary_exit::validate_voluntary_exit,
    },
    p2p_sender::P2PSender,
};

pub fn init_gossipsub_config_with_topics() -> GossipsubConfig {
    let mut gossipsub_config = GossipsubConfig::default();
    let fork_digest = beacon_network_spec().fork_digest(FULU_FORK_EPOCH, genesis_validators_root());

    let mut topics = vec![
        GossipTopic {
            fork: fork_digest,
            kind: GossipTopicKind::BeaconBlock,
        },
        GossipTopic {
            fork: fork_digest,
            kind: GossipTopicKind::AggregateAndProof,
        },
        GossipTopic {
            fork: fork_digest,
            kind: GossipTopicKind::VoluntaryExit,
        },
        GossipTopic {
            fork: fork_digest,
            kind: GossipTopicKind::ProposerSlashing,
        },
        GossipTopic {
            fork: fork_digest,
            kind: GossipTopicKind::AttesterSlashing,
        },
        GossipTopic {
            fork: fork_digest,
            kind: GossipTopicKind::SyncCommitteeContributionAndProof,
        },
        GossipTopic {
            fork: fork_digest,
            kind: GossipTopicKind::BlsToExecutionChange,
        },
        GossipTopic {
            fork: fork_digest,
            kind: GossipTopicKind::LightClientFinalityUpdate,
        },
        GossipTopic {
            fork: fork_digest,
            kind: GossipTopicKind::LightClientOptimisticUpdate,
        },
    ];

    // Subnets
    for subnet_id in 0..beacon_network_spec().attestation_subnet_count {
        topics.push(GossipTopic {
            fork: fork_digest,
            kind: GossipTopicKind::BeaconAttestation(subnet_id),
        });
    }

    for subnet_id in 0..SYNC_COMMITTEE_SUBNET_COUNT {
        topics.push(GossipTopic {
            fork: fork_digest,
            kind: GossipTopicKind::SyncCommittee(subnet_id),
        });
    }

    for subnet_id in 0..beacon_network_spec().blob_sidecar_subnet_count_electra {
        topics.push(GossipTopic {
            fork: fork_digest,
            kind: GossipTopicKind::BlobSidecar(subnet_id),
        });
    }

    for subnet_id in 0..DATA_COLUMN_SIDECAR_SUBNET_COUNT {
        topics.push(GossipTopic {
            fork: fork_digest,
            kind: GossipTopicKind::DataColumnSidecar(subnet_id),
        });
    }

    gossipsub_config.set_topics(topics);

    gossipsub_config
}

async fn import_gossip_attestation(
    beacon_chain: &BeaconChain,
    single_attestation: &SingleAttestation,
) -> anyhow::Result<()> {
    let (attestation, should_process_attestation) = {
        let store = beacon_chain.store.lock().await;
        let head_root = store.get_head()?;
        let state =
            store.db.state_provider().get(head_root)?.ok_or_else(|| {
                anyhow::anyhow!("No beacon state found for head root: {head_root}")
            })?;
        let attestation = single_attestation_to_attestation(single_attestation, &state)?;

        store
            .operation_pool
            .insert_attestation(attestation.clone(), single_attestation.committee_index);

        let current_slot = store.get_current_slot()?;
        info!(
            current_slot,
            attestation_slot = single_attestation.data.slot,
            committee_index = single_attestation.committee_index,
            attester_index = single_attestation.attester_index,
            beacon_block_root = %single_attestation.data.beacon_block_root,
            target_epoch = single_attestation.data.target.epoch,
            target_root = %single_attestation.data.target.root,
            "beacon_e2e_trace: gossip attestation inserted"
        );
        (
            attestation,
            current_slot >= single_attestation.data.slot + MIN_ATTESTATION_INCLUSION_DELAY,
        )
    };

    if should_process_attestation {
        info!(
            attestation_slot = attestation.data.slot,
            beacon_block_root = %attestation.data.beacon_block_root,
            target_epoch = attestation.data.target.epoch,
            target_root = %attestation.data.target.root,
            "beacon_e2e_trace: gossip attestation processing fork choice"
        );
        beacon_chain.process_attestation(attestation, false).await?;
    }

    Ok(())
}

fn report_gossip_validation_result(
    p2p_sender: &P2PSender,
    message_id: &MessageId,
    propagation_source: &PeerId,
    validation_result: &ValidationResult,
) {
    let acceptance = match validation_result {
        ValidationResult::Accept => MessageAcceptance::Accept,
        ValidationResult::Ignore(_) => MessageAcceptance::Ignore,
        ValidationResult::Reject(_) => MessageAcceptance::Reject,
    };

    p2p_sender.report_gossip_validation(message_id.clone(), *propagation_source, acceptance);
}

fn accept_gossip_message(
    p2p_sender: &P2PSender,
    message_id: &MessageId,
    propagation_source: &PeerId,
) {
    p2p_sender.report_gossip_validation(
        message_id.clone(),
        *propagation_source,
        MessageAcceptance::Accept,
    );
}

/// Dispatches a gossipsub message to its appropriate handler.
pub async fn handle_gossipsub_message(
    propagation_source: PeerId,
    message_id: MessageId,
    message: Message,
    beacon_chain: &BeaconChain,
    cached_db: &BeaconCacheDB,
    p2p_sender: &P2PSender,
) {
    match GossipsubMessage::decode(&message.topic, &message.data) {
        Ok(gossip_message) => match gossip_message {
            GossipsubMessage::BeaconBlock(signed_block) => {
                let slot = signed_block.message.slot;
                let root = signed_block.message.block_root();
                let parent_root = signed_block.message.parent_root;
                let proposer_index = signed_block.message.proposer_index;
                let attestation_count = signed_block.message.body.attestations.len();
                info!(
                    slot,
                    %root,
                    %parent_root,
                    proposer_index,
                    attestation_count,
                    "beacon_e2e_trace: gossip block received"
                );

                let tick_time = {
                    let duration = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .expect("System time is before UNIX epoch");
                    duration.as_secs() + u64::from(duration.subsec_nanos() > 0)
                };
                if let Err(err) = beacon_chain.process_tick(tick_time).await {
                    warn!("Failed to process gossipsub tick before block validation: {err}");
                    return;
                }

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
                        accept_gossip_message(p2p_sender, &message_id, &propagation_source);
                        info!(
                            slot,
                            %root,
                            %parent_root,
                            proposer_index,
                            attestation_count,
                            "beacon_e2e_trace: gossip block accepted"
                        );
                        if let Err(err) = beacon_chain.process_block(*signed_block).await {
                            error!(
                                slot,
                                %root,
                                %parent_root,
                                "beacon_e2e_trace: gossip block import failed: {err}"
                            );
                        } else {
                            info!(
                                slot,
                                %root,
                                %parent_root,
                                "beacon_e2e_trace: gossip block imported"
                            );
                        }
                    }
                    ValidationResult::Ignore(reason) => {
                        report_gossip_validation_result(
                            p2p_sender,
                            &message_id,
                            &propagation_source,
                            &ValidationResult::Ignore(reason.clone()),
                        );
                        warn!(
                            slot,
                            %root,
                            %parent_root,
                            reason,
                            "beacon_e2e_trace: gossip block ignored"
                        );
                    }
                    ValidationResult::Reject(reason) => {
                        report_gossip_validation_result(
                            p2p_sender,
                            &message_id,
                            &propagation_source,
                            &ValidationResult::Reject(reason.clone()),
                        );
                        warn!(
                            slot,
                            %root,
                            %parent_root,
                            reason,
                            "beacon_e2e_trace: gossip block rejected"
                        );
                    }
                }
            }
            GossipsubMessage::BeaconAttestation((single_attestation, subnet_id)) => {
                trace!(
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
                            accept_gossip_message(p2p_sender, &message_id, &propagation_source);
                            if let Err(err) =
                                import_gossip_attestation(beacon_chain, &single_attestation).await
                            {
                                warn!("Failed to import gossipsub beacon attestation: {err}");
                            }
                        }
                        ValidationResult::Reject(reason) => {
                            report_gossip_validation_result(
                                p2p_sender,
                                &message_id,
                                &propagation_source,
                                &ValidationResult::Reject(reason.clone()),
                            );
                            info!("Attestation rejected: {reason}");
                        }
                        ValidationResult::Ignore(reason) => {
                            report_gossip_validation_result(
                                p2p_sender,
                                &message_id,
                                &propagation_source,
                                &ValidationResult::Ignore(reason.clone()),
                            );
                            info!("Attestation ignored: {reason}");
                        }
                    },
                    Err(err) => {
                        trace!("Could not validate attestation: {err}");
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
                            accept_gossip_message(p2p_sender, &message_id, &propagation_source);
                        }
                        ValidationResult::Reject(reason) => {
                            report_gossip_validation_result(
                                p2p_sender,
                                &message_id,
                                &propagation_source,
                                &ValidationResult::Reject(reason.clone()),
                            );
                            info!("BLS to Execution Change rejected: {reason}");
                        }
                        ValidationResult::Ignore(reason) => {
                            report_gossip_validation_result(
                                p2p_sender,
                                &message_id,
                                &propagation_source,
                                &ValidationResult::Ignore(reason.clone()),
                            );
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

                match validate_aggregate_and_proof(&aggregate_and_proof, beacon_chain, cached_db)
                    .await
                {
                    Ok(validation_result) => match validation_result {
                        ValidationResult::Accept => {
                            accept_gossip_message(p2p_sender, &message_id, &propagation_source);
                        }
                        ValidationResult::Reject(reason) => {
                            report_gossip_validation_result(
                                p2p_sender,
                                &message_id,
                                &propagation_source,
                                &ValidationResult::Reject(reason.clone()),
                            );
                            info!("Aggregate and proof rejected: {reason}");
                        }
                        ValidationResult::Ignore(reason) => {
                            report_gossip_validation_result(
                                p2p_sender,
                                &message_id,
                                &propagation_source,
                                &ValidationResult::Ignore(reason.clone()),
                            );
                            info!("Aggregate and proof ignored: {reason}");
                        }
                    },
                    Err(err) => {
                        error!("Could not validate aggregate and proof: {err}");
                    }
                }
            }
            GossipsubMessage::SyncCommittee((sync_committee, subnet_id)) => {
                trace!(
                    "Sync Committee received over gossipsub: root: {}",
                    sync_committee.tree_hash_root()
                );

                match validate_sync_committee(&sync_committee, beacon_chain, subnet_id, cached_db)
                    .await
                {
                    Ok(validation_result) => match validation_result {
                        ValidationResult::Accept => {
                            accept_gossip_message(p2p_sender, &message_id, &propagation_source);
                        }
                        ValidationResult::Reject(reason) => {
                            report_gossip_validation_result(
                                p2p_sender,
                                &message_id,
                                &propagation_source,
                                &ValidationResult::Reject(reason.clone()),
                            );
                            info!("Sync committee message rejected: {reason}");
                        }
                        ValidationResult::Ignore(reason) => {
                            report_gossip_validation_result(
                                p2p_sender,
                                &message_id,
                                &propagation_source,
                                &ValidationResult::Ignore(reason.clone()),
                            );
                            trace!("Sync committee message ignored: {reason}");
                        }
                    },
                    Err(err) => {
                        error!("Could not validate sync committee message: {err}");
                    }
                }
            }
            GossipsubMessage::SyncCommitteeContributionAndProof(signed_contribution_and_proof) => {
                info!(
                    "Sync Committee Contribution And Proof received over gossipsub: root: {}",
                    signed_contribution_and_proof.tree_hash_root()
                );

                match validate_sync_committee_contribution_and_proof(
                    beacon_chain,
                    cached_db,
                    &signed_contribution_and_proof,
                )
                .await
                {
                    Ok(validation_result) => match validation_result {
                        ValidationResult::Accept => {
                            accept_gossip_message(p2p_sender, &message_id, &propagation_source);
                        }

                        ValidationResult::Reject(reason) => {
                            report_gossip_validation_result(
                                p2p_sender,
                                &message_id,
                                &propagation_source,
                                &ValidationResult::Reject(reason.clone()),
                            );
                            info!("Sync committee contribution and proof rejected: {reason}");
                        }
                        ValidationResult::Ignore(reason) => {
                            report_gossip_validation_result(
                                p2p_sender,
                                &message_id,
                                &propagation_source,
                                &ValidationResult::Ignore(reason.clone()),
                            );
                            info!("Sync committee contribution and proof ignored: {reason}");
                        }
                    },
                    Err(err) => {
                        error!("Could not validate sync committee contribution and proof: {err}");
                    }
                }
            }
            GossipsubMessage::AttesterSlashing(attester_slashing) => {
                info!(
                    "Attester Slashing received over gossipsub: root: {}",
                    attester_slashing.tree_hash_root()
                );

                match validate_attester_slashing(&attester_slashing, beacon_chain, cached_db).await
                {
                    Ok(validation_result) => match validation_result {
                        ValidationResult::Accept => {
                            accept_gossip_message(p2p_sender, &message_id, &propagation_source);
                            if let Err(err) = beacon_chain
                                .process_attester_slashing(*attester_slashing)
                                .await
                            {
                                error!("Failed to process gossipsub attester slashing: {err}");
                            }
                        }
                        ValidationResult::Reject(reason) => {
                            report_gossip_validation_result(
                                p2p_sender,
                                &message_id,
                                &propagation_source,
                                &ValidationResult::Reject(reason.clone()),
                            );
                            info!("Attester slashing rejected: {reason}");
                        }
                        ValidationResult::Ignore(reason) => {
                            report_gossip_validation_result(
                                p2p_sender,
                                &message_id,
                                &propagation_source,
                                &ValidationResult::Ignore(reason.clone()),
                            );
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
                            accept_gossip_message(p2p_sender, &message_id, &propagation_source);
                        }
                        ValidationResult::Reject(reason) => {
                            report_gossip_validation_result(
                                p2p_sender,
                                &message_id,
                                &propagation_source,
                                &ValidationResult::Reject(reason.clone()),
                            );
                            info!("Proposer slashing rejected: {reason}");
                        }
                        ValidationResult::Ignore(reason) => {
                            report_gossip_validation_result(
                                p2p_sender,
                                &message_id,
                                &propagation_source,
                                &ValidationResult::Ignore(reason.clone()),
                            );
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
                            accept_gossip_message(p2p_sender, &message_id, &propagation_source);
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
                        }
                        ValidationResult::Reject(reason) => {
                            report_gossip_validation_result(
                                p2p_sender,
                                &message_id,
                                &propagation_source,
                                &ValidationResult::Reject(reason.clone()),
                            );
                            info!("Blob_sidecar rejected: {reason}");
                        }
                        ValidationResult::Ignore(reason) => {
                            report_gossip_validation_result(
                                p2p_sender,
                                &message_id,
                                &propagation_source,
                                &ValidationResult::Ignore(reason.clone()),
                            );
                            info!("Blob_sidecar ignored: {reason}");
                        }
                    },
                    Err(err) => {
                        error!("Could not validate blob_sidecar: {err}");
                    }
                }
            }
            GossipsubMessage::DataColumnSidecar(data_column_sidecar) => {
                info!(
                    "Data Column Sidecar received over gossipsub: index: {}, root: {}",
                    data_column_sidecar.index,
                    data_column_sidecar
                        .signed_block_header
                        .message
                        .tree_hash_root()
                );

                // Extract subnet_id from the gossip topic
                let subnet_id = match GossipTopic::from_topic_hash(&message.topic) {
                    Ok(topic) => match topic.kind {
                        GossipTopicKind::DataColumnSidecar(id) => id,
                        _ => {
                            error!("Unexpected topic kind for data column sidecar");
                            return;
                        }
                    },
                    Err(err) => {
                        error!("Failed to parse topic for data column sidecar: {err}");
                        return;
                    }
                };

                let validation_result = match validate_data_column_sidecar_full(
                    &data_column_sidecar,
                    beacon_chain,
                    subnet_id,
                    cached_db,
                )
                .await
                {
                    Ok(validation_result) => validation_result,
                    Err(err) => {
                        error!("Could not validate data_column_sidecar: {err}");
                        return;
                    }
                };

                match validation_result {
                    ValidationResult::Accept => {
                        accept_gossip_message(p2p_sender, &message_id, &propagation_source);
                        if let Err(err) = beacon_chain
                            .store
                            .lock()
                            .await
                            .db
                            .column_sidecars_provider()
                            .insert(
                                ColumnIdentifier::new(
                                    data_column_sidecar
                                        .signed_block_header
                                        .message
                                        .tree_hash_root(),
                                    data_column_sidecar.index,
                                ),
                                *data_column_sidecar,
                            )
                        {
                            error!("Failed to insert data_column_sidecar: {err}");
                        }
                    }
                    ValidationResult::Reject(reason) => {
                        report_gossip_validation_result(
                            p2p_sender,
                            &message_id,
                            &propagation_source,
                            &ValidationResult::Reject(reason.clone()),
                        );
                        info!("Data column sidecar rejected: {reason}");
                    }
                    ValidationResult::Ignore(reason) => {
                        report_gossip_validation_result(
                            p2p_sender,
                            &message_id,
                            &propagation_source,
                            &ValidationResult::Ignore(reason.clone()),
                        );
                        info!("Data column sidecar ignored: {reason}");
                    }
                }
            }
            GossipsubMessage::LightClientFinalityUpdate(light_client_finality_update) => {
                info!(
                    "Light Client Finality Update received over gossipsub: root: {}",
                    light_client_finality_update.tree_hash_root()
                );

                match validate_light_client_finality_update(
                    &light_client_finality_update,
                    cached_db,
                )
                .await
                {
                    Ok(validation_result) => match validation_result {
                        ValidationResult::Accept => {
                            accept_gossip_message(p2p_sender, &message_id, &propagation_source);
                        }
                        ValidationResult::Reject(reason) => {
                            report_gossip_validation_result(
                                p2p_sender,
                                &message_id,
                                &propagation_source,
                                &ValidationResult::Reject(reason.clone()),
                            );
                            info!("Light client finality update rejected: {reason}");
                        }
                        ValidationResult::Ignore(reason) => {
                            report_gossip_validation_result(
                                p2p_sender,
                                &message_id,
                                &propagation_source,
                                &ValidationResult::Ignore(reason.clone()),
                            );
                            info!("Light client finality update ignored: {reason}");
                        }
                    },
                    Err(err) => {
                        error!("Could not validate light client finality update: {err}");
                    }
                }
            }
            GossipsubMessage::LightClientOptimisticUpdate(light_client_optimistic_update) => {
                info!(
                    "Light Client Optimistic Update received over gossipsub: root: {}",
                    light_client_optimistic_update.tree_hash_root()
                );

                match validate_light_client_optimistic_update(
                    &light_client_optimistic_update,
                    beacon_chain,
                    cached_db,
                )
                .await
                {
                    Ok(validation_result) => match validation_result {
                        ValidationResult::Accept => {
                            accept_gossip_message(p2p_sender, &message_id, &propagation_source);

                            *cached_db.forwarded_optimistic_update_slot.write().await =
                                Some(light_client_optimistic_update.attested_header.beacon.slot);
                        }
                        ValidationResult::Ignore(reason) => {
                            report_gossip_validation_result(
                                p2p_sender,
                                &message_id,
                                &propagation_source,
                                &ValidationResult::Ignore(reason.clone()),
                            );
                            info!("Light client optimistic update ignored: {reason}");
                        }
                        ValidationResult::Reject(reason) => {
                            report_gossip_validation_result(
                                p2p_sender,
                                &message_id,
                                &propagation_source,
                                &ValidationResult::Reject(reason.clone()),
                            );
                            info!("Light client optimistic update rejected: {reason}");
                        }
                    },
                    Err(err) => {
                        error!("Could not validate light client optimistic update: {err}");
                    }
                }
            }
            GossipsubMessage::VoluntaryExit(voluntary_exit) => {
                info!(
                    "Voluntary Exit received over gossipsub: root: {}",
                    voluntary_exit.tree_hash_root()
                );

                match validate_voluntary_exit(&voluntary_exit, beacon_chain, cached_db).await {
                    Ok(validation_result) => match validation_result {
                        ValidationResult::Accept => {
                            accept_gossip_message(p2p_sender, &message_id, &propagation_source);
                        }
                        ValidationResult::Reject(reason) => {
                            report_gossip_validation_result(
                                p2p_sender,
                                &message_id,
                                &propagation_source,
                                &ValidationResult::Reject(reason.clone()),
                            );
                            info!("voluntary_exit rejected: {reason}");
                        }
                        ValidationResult::Ignore(reason) => {
                            report_gossip_validation_result(
                                p2p_sender,
                                &message_id,
                                &propagation_source,
                                &ValidationResult::Ignore(reason.clone()),
                            );
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
