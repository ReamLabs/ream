use std::sync::Arc;

use libp2p::gossipsub::TopicHash;
use ream_consensus_lean::{block::SignedBlock, vote::SignedVote};
use ssz::Decode;

use super::topics::{LeanGossipTopic, LeanGossipTopicKind};
use crate::gossipsub::error::GossipsubError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LeanGossipsubMessage {
    Block(Arc<SignedBlock>),
    Vote(Arc<SignedVote>),
}

impl LeanGossipsubMessage {
    pub fn decode(topic: &TopicHash, data: &[u8]) -> Result<Self, GossipsubError> {
        match LeanGossipTopic::from_topic_hash(topic)?.kind {
            LeanGossipTopicKind::Block => {
                Ok(Self::Block(Arc::new(SignedBlock::from_ssz_bytes(data)?)))
            }
            LeanGossipTopicKind::Vote => {
                Ok(Self::Vote(Arc::new(SignedVote::from_ssz_bytes(data)?)))
            }
        }
    }
}
