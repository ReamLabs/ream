use libp2p::gossipsub::TopicHash;
use ream_consensus::{
    attestation::Attestation, attester_slashing::AttesterSlashing, blob_sidecar::BlobSidecar,
    constants::genesis_validators_root, electra::beacon_block::SignedBeaconBlock,
};
use ream_network_spec::networks::network_spec;
use ream_validator::aggregate_and_proof::AggregateAndProof;
use ssz::Decode;

use super::{
    error::GossipsubError,
    topics::{GossipTopic, GossipTopicKind},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GossipsubMessage {
    BeaconBlock(Box<SignedBeaconBlock>),
    AttesterSlashing(Box<AttesterSlashing>),
    AggregateAndProof(Box<AggregateAndProof>),
    BlobSidecar(Box<BlobSidecar>),
    BeaconAttestation(Box<Attestation>),
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
            GossipTopicKind::AggregateAndProof => Ok(Self::AggregateAndProof(Box::new(
                AggregateAndProof::from_ssz_bytes(data)?,
            ))),
            GossipTopicKind::BeaconAttestation(_) => Ok(Self::BeaconAttestation(Box::new(
                Attestation::from_ssz_bytes(data)?,
            ))),
            GossipTopicKind::AttesterSlashing => Ok(Self::AttesterSlashing(Box::new(
                AttesterSlashing::from_ssz_bytes(data)?,
            ))),
            GossipTopicKind::BlobSidecar(_) => Ok(Self::BlobSidecar(Box::new(
                BlobSidecar::from_ssz_bytes(data)?,
            ))),
            _ => Err(GossipsubError::InvalidTopic(format!(
                "Topic not supported: {topic:?}"
            ))),
        }
    }
}
