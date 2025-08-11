use libp2p::gossipsub::TopicHash;
use ream_consensus_lean::{block::SignedBlock, vote::SignedVote};
use ssz::Decode;

use super::topics::{LeanGossipTopic, LeanGossipTopicKind};
use crate::gossipsub::error::GossipsubError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LeanGossipsubMessage {
    Block(Box<SignedBlock>),
    Vote(Box<SignedVote>),
}

impl LeanGossipsubMessage {
    pub fn decode(topic: &TopicHash, data: &[u8]) -> Result<Self, GossipsubError> {
        match LeanGossipTopic::from_topic_hash(topic)?.kind {
            LeanGossipTopicKind::LeanBlock => {
                Ok(Self::Block(Box::new(SignedBlock::from_ssz_bytes(data)?)))
            }
            LeanGossipTopicKind::LeanVote => {
                Ok(Self::Vote(Box::new(SignedVote::from_ssz_bytes(data)?)))
            }
        }
    }
}
