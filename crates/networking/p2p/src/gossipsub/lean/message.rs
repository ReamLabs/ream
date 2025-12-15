use libp2p::gossipsub::TopicHash;
use ream_consensus_lean::{attestation::SignedAttestation, block::SignedBlockWithAttestation};
use ssz::Decode;

use super::topics::{LeanGossipTopic, LeanGossipTopicKind};
use crate::gossipsub::error::GossipsubError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LeanGossipsubMessage {
    Block(Box<SignedBlockWithAttestation>),
    Attestation(Box<SignedAttestation>),
}

impl LeanGossipsubMessage {
    pub fn decode(topic: &TopicHash, data: &[u8]) -> Result<Self, GossipsubError> {
        match LeanGossipTopic::from_topic_hash(topic)?.kind {
            LeanGossipTopicKind::Block => Ok(Self::Block(Box::new(
                SignedBlockWithAttestation::from_ssz_bytes(data)?,
            ))),
            LeanGossipTopicKind::Attestation => Ok(Self::Attestation(Box::new(
                SignedAttestation::from_ssz_bytes(data)?,
            ))),
        }
    }
}
