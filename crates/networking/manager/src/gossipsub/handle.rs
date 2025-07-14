use libp2p::gossipsub::Message;
use ream_beacon_chain::beacon_chain::BeaconChain;
use ream_consensus::constants::genesis_validators_root;
use ream_network_spec::networks::network_spec;
use ream_p2p::gossipsub::{
    configurations::GossipsubConfig,
    message::GossipsubMessage,
    topics::{GossipTopic, GossipTopicKind},
};
use ream_storage::cache::CachedDB;
use tracing::{error, info, trace};
use tree_hash::TreeHash;

use crate::p2p_sender::P2PSender;

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
    _cached_db: &CachedDB,
    _p2psender: &P2PSender,
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
