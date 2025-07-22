use libp2p::gossipsub::Message;
use ream_beacon_chain::beacon_chain::BeaconChain;
use ream_consensus_beacon::{
    blob_sidecar::BlobIdentifier, execution_engine::rpc_types::get_blobs::BlobAndProofV1,
};
use ream_consensus_misc::constants::genesis_validators_root;
use ream_network_spec::networks::network_spec;
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
use tracing::{error, info, trace};
use tree_hash::TreeHash;

use crate::{
    gossipsub::validate::{blob_sidecar::validate_blob_sidecar, result::ValidationResult},
    p2p_sender::P2PSender,
};

pub fn init_gossipsub_config_with_topics() -> GossipsubConfig {
    let mut gossipsub_config = GossipsubConfig::default();

    gossipsub_config.set_topics(vec![
        GossipTopic {
            fork: network_spec().fork_digest(genesis_validators_root()),
            kind: GossipTopicKind::BeaconBlock,
        },
        GossipTopic {
            fork: network_spec().fork_digest(genesis_validators_root()),
            kind: GossipTopicKind::AggregateAndProof,
        },
        GossipTopic {
            fork: network_spec().fork_digest(genesis_validators_root()),
            kind: GossipTopicKind::VoluntaryExit,
        },
        GossipTopic {
            fork: network_spec().fork_digest(genesis_validators_root()),
            kind: GossipTopicKind::ProposerSlashing,
        },
        GossipTopic {
            fork: network_spec().fork_digest(genesis_validators_root()),
            kind: GossipTopicKind::AttesterSlashing,
        },
        GossipTopic {
            fork: network_spec().fork_digest(genesis_validators_root()),
            kind: GossipTopicKind::BeaconAttestation(0),
        },
        GossipTopic {
            fork: network_spec().fork_digest(genesis_validators_root()),
            kind: GossipTopicKind::SyncCommittee(0),
        },
        GossipTopic {
            fork: network_spec().fork_digest(genesis_validators_root()),
            kind: GossipTopicKind::SyncCommitteeContributionAndProof,
        },
        GossipTopic {
            fork: network_spec().fork_digest(genesis_validators_root()),
            kind: GossipTopicKind::BlsToExecutionChange,
        },
        GossipTopic {
            fork: network_spec().fork_digest(genesis_validators_root()),
            kind: GossipTopicKind::LightClientFinalityUpdate,
        },
        GossipTopic {
            fork: network_spec().fork_digest(genesis_validators_root()),
            kind: GossipTopicKind::LightClientOptimisticUpdate,
        },
        GossipTopic {
            fork: network_spec().fork_digest(genesis_validators_root()),
            kind: GossipTopicKind::BlobSidecar(0),
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
            }
            GossipsubMessage::BeaconAttestation(attestation) => {
                info!(
                    "Beacon Attestation received over gossipsub: root: {}",
                    attestation.tree_hash_root()
                );

                if let Err(err) = beacon_chain.process_attestation(*attestation, true).await {
                    error!("Failed to process gossipsub attestation: {err}");
                }
            }
            GossipsubMessage::BlsToExecutionChange(bls_to_execution_change) => {
                info!(
                    "Bls To Execution Change received over gossipsub: root: {}",
                    bls_to_execution_change.tree_hash_root()
                );
            }
            GossipsubMessage::AggregateAndProof(aggregate_and_proof) => {
                info!(
                    "Aggregate And Proof received over gossipsub: root: {}",
                    aggregate_and_proof.tree_hash_root()
                );
            }
            GossipsubMessage::SyncCommittee(sync_committee) => {
                info!(
                    "Sync Committee received over gossipsub: root: {}",
                    sync_committee.tree_hash_root()
                );
            }
            GossipsubMessage::SyncCommitteeContributionAndProof(
                _sync_committee_contribution_and_proof,
            ) => {}
            GossipsubMessage::AttesterSlashing(attester_slashing) => {
                info!(
                    "Attester Slashing received over gossipsub: root: {}",
                    attester_slashing.tree_hash_root()
                );

                if let Err(err) = beacon_chain
                    .process_attester_slashing(*attester_slashing)
                    .await
                {
                    error!("Failed to process gossipsub attester slashing: {err}");
                }
            }
            GossipsubMessage::ProposerSlashing(proposer_slashing) => {
                info!(
                    "Proposer Slashing received over gossipsub: root: {}",
                    proposer_slashing.tree_hash_root()
                );
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
        },
        Err(err) => {
            trace!("Failed to decode gossip message: {err:?}");
        }
    };
}
