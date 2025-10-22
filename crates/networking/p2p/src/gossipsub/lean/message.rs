use libp2p::gossipsub::TopicHash;
use ream_consensus_lean::{attestation::SignedAttestation, block::SignedBlock};
use ssz::Decode;

use super::topics::{LeanGossipTopic, LeanGossipTopicKind};
use crate::gossipsub::error::GossipsubError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LeanGossipsubMessage {
    Block(SignedBlock),
    Attestation(SignedAttestation),
}

impl LeanGossipsubMessage {
    pub fn decode(topic: &TopicHash, data: &[u8]) -> Result<Self, GossipsubError> {
        match LeanGossipTopic::from_topic_hash(topic)?.kind {
            LeanGossipTopicKind::Block => Ok(Self::Block(SignedBlock::from_ssz_bytes(data)?)),
            LeanGossipTopicKind::Vote => {
                Ok(Self::Attestation(SignedAttestation::from_ssz_bytes(data)?))
            }
        }
    }
}
