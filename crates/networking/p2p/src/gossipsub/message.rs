use libp2p::gossipsub::TopicHash;
use ream_consensus::{
    constants::genesis_validators_root, electra::beacon_block::SignedBeaconBlock,
};
use ream_network_spec::networks::network_spec;
use ssz::Decode;

use super::{
    error::GossipsubError,
    topics::{GossipTopic, GossipTopicKind},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GossipsubMessage {
    BeaconBlock(SignedBeaconBlock),
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
            GossipTopicKind::BeaconBlock => {
                Ok(Self::BeaconBlock(SignedBeaconBlock::from_ssz_bytes(data)?))
            }
            _ => Err(GossipsubError::InvalidTopic(format!(
                "Topic not supported: {topic:?}"
            ))),
        }
    }
}
