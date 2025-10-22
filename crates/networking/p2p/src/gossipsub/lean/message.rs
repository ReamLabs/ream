use libp2p::gossipsub::TopicHash;
use ream_consensus_lean::{block::SignedBlock, vote::SignedValidatorAttestation};
use ssz::Decode;

use super::topics::{LeanGossipTopic, LeanGossipTopicKind};
use crate::gossipsub::error::GossipsubError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LeanGossipsubMessage {
    Block(SignedBlock),
    Vote(SignedValidatorAttestation),
}

impl LeanGossipsubMessage {
    pub fn decode(topic: &TopicHash, data: &[u8]) -> Result<Self, GossipsubError> {
        match LeanGossipTopic::from_topic_hash(topic)?.kind {
            LeanGossipTopicKind::Block => Ok(Self::Block(SignedBlock::from_ssz_bytes(data)?)),
            LeanGossipTopicKind::Vote => Ok(Self::Vote(
                SignedValidatorAttestation::from_ssz_bytes(data)?,
            )),
        }
    }
}
