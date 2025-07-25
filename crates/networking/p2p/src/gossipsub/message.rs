use libp2p::gossipsub::TopicHash;
use ream_consensus_beacon::{
    attester_slashing::AttesterSlashing, blob_sidecar::BlobSidecar,
    bls_to_execution_change::BLSToExecutionChange, electra::beacon_block::SignedBeaconBlock,
    proposer_slashing::ProposerSlashing, single_attestation::SingleAttestation,
    sync_committee::SyncCommittee,
};
use ream_consensus_misc::constants::genesis_validators_root;
use ream_light_client::{
    finality_update::LightClientFinalityUpdate, optimistic_update::LightClientOptimisticUpdate,
};
use ream_network_spec::networks::network_spec;
use ream_validator_beacon::{
    aggregate_and_proof::AggregateAndProof, contribution_and_proof::SignedContributionAndProof,
};
use ssz::Decode;

use super::{
    error::GossipsubError,
    topics::{GossipTopic, GossipTopicKind},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GossipsubMessage {
    BeaconBlock(Box<SignedBeaconBlock>),
    AttesterSlashing(Box<AttesterSlashing>),
    ProposerSlashing(Box<ProposerSlashing>),
    AggregateAndProof(Box<AggregateAndProof>),
    BlobSidecar(Box<BlobSidecar>),
    BeaconAttestation((Box<SingleAttestation>, u64)),
    SyncCommittee(Box<SyncCommittee>),
    BlsToExecutionChange(Box<BLSToExecutionChange>),
    SyncCommitteeContributionAndProof(Box<SignedContributionAndProof>),
    LightClientFinalityUpdate(Box<LightClientFinalityUpdate>),
    LightClientOptimisticUpdate(Box<LightClientOptimisticUpdate>),
}

impl GossipsubMessage {
    pub fn decode(topic: &TopicHash, data: &[u8]) -> Result<Self, GossipsubError> {
        let gossip_topic = GossipTopic::from_topic_hash(topic)?;

        if gossip_topic.fork != network_spec().fork_digest(genesis_validators_root()) {
            return Err(GossipsubError::InvalidTopic(format!(
                "Invalid topic fork: {topic:?}"
            )));
        }

        match gossip_topic.kind {
            GossipTopicKind::BeaconBlock => Ok(Self::BeaconBlock(Box::new(
                SignedBeaconBlock::from_ssz_bytes(data)?,
            ))),
            GossipTopicKind::SyncCommittee(_) => Ok(Self::SyncCommittee(Box::new(
                SyncCommittee::from_ssz_bytes(data)?,
            ))),
            GossipTopicKind::SyncCommitteeContributionAndProof => {
                Ok(Self::SyncCommitteeContributionAndProof(Box::new(
                    SignedContributionAndProof::from_ssz_bytes(data)?,
                )))
            }
            GossipTopicKind::AggregateAndProof => Ok(Self::AggregateAndProof(Box::new(
                AggregateAndProof::from_ssz_bytes(data)?,
            ))),
            GossipTopicKind::BeaconAttestation(subnet_id) => Ok(Self::BeaconAttestation((
                Box::new(SingleAttestation::from_ssz_bytes(data)?),
                subnet_id,
            ))),
            GossipTopicKind::BlsToExecutionChange => Ok(Self::BlsToExecutionChange(Box::new(
                BLSToExecutionChange::from_ssz_bytes(data)?,
            ))),
            GossipTopicKind::AttesterSlashing => Ok(Self::AttesterSlashing(Box::new(
                AttesterSlashing::from_ssz_bytes(data)?,
            ))),
            GossipTopicKind::ProposerSlashing => Ok(Self::ProposerSlashing(Box::new(
                ProposerSlashing::from_ssz_bytes(data)?,
            ))),
            GossipTopicKind::BlobSidecar(_) => Ok(Self::BlobSidecar(Box::new(
                BlobSidecar::from_ssz_bytes(data)?,
            ))),
            GossipTopicKind::LightClientFinalityUpdate => Ok(Self::LightClientFinalityUpdate(
                Box::new(LightClientFinalityUpdate::from_ssz_bytes(data)?),
            )),
            GossipTopicKind::LightClientOptimisticUpdate => Ok(Self::LightClientOptimisticUpdate(
                Box::new(LightClientOptimisticUpdate::from_ssz_bytes(data)?),
            )),
            _ => Err(GossipsubError::InvalidTopic(format!(
                "Topic not supported: {topic:?}"
            ))),
        }
    }
}
