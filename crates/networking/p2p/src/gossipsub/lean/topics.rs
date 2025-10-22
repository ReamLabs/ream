use LeanGossipTopicKind::*;
use libp2p::gossipsub::{IdentTopic as Topic, TopicHash};

use crate::gossipsub::error::GossipsubError;

pub const TOPIC_PREFIX: &str = "leanconsensus";
pub const ENCODING_POSTFIX: &str = "ssz_snappy";
pub const LEAN_BLOCK_TOPIC: &str = "block";
pub const LEAN_ATTESTATION_TOPIC: &str = "attestation";

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct LeanGossipTopic {
    pub fork: String,
    pub kind: LeanGossipTopicKind,
}

impl LeanGossipTopic {
    pub fn from_topic_hash(topic: &TopicHash) -> Result<Self, GossipsubError> {
        let topic_parts: Vec<&str> = topic.as_str().trim_start_matches('/').split('/').collect();

        if topic_parts.len() != 4
            || topic_parts[0] != TOPIC_PREFIX
            || topic_parts[3] != ENCODING_POSTFIX
        {
            return Err(GossipsubError::InvalidTopic(format!(
                "Invalid topic format: {topic:?}"
            )));
        }

        let fork = topic_parts[1].to_string();
        let kind = match topic_parts[2] {
            LEAN_BLOCK_TOPIC => LeanGossipTopicKind::Block,
            LEAN_ATTESTATION_TOPIC => LeanGossipTopicKind::Attestation,
            other => {
                return Err(GossipsubError::InvalidTopic(format!(
                    "Invalid topic: {other:?}"
                )));
            }
        };

        Ok(LeanGossipTopic { fork, kind })
    }
}

impl std::fmt::Display for LeanGossipTopic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "/{TOPIC_PREFIX}/{}/{}/{ENCODING_POSTFIX}",
            self.fork, self.kind,
        )
    }
}

impl From<LeanGossipTopic> for Topic {
    fn from(topic: LeanGossipTopic) -> Topic {
        Topic::new(topic)
    }
}

impl From<LeanGossipTopic> for String {
    fn from(topic: LeanGossipTopic) -> Self {
        topic.to_string()
    }
}

impl From<LeanGossipTopic> for TopicHash {
    fn from(val: LeanGossipTopic) -> Self {
        let kind_str = match &val.kind {
            Block => LEAN_BLOCK_TOPIC,
            Attestation => LEAN_ATTESTATION_TOPIC,
        };
        TopicHash::from_raw(format!(
            "/{TOPIC_PREFIX}/{}/{kind_str}/{ENCODING_POSTFIX}",
            val.fork,
        ))
    }
}

#[derive(Debug, Hash, Clone, Copy, PartialEq, Eq)]
pub enum LeanGossipTopicKind {
    Block,
    Attestation,
}

impl std::fmt::Display for LeanGossipTopicKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LeanGossipTopicKind::Block => write!(f, "{LEAN_BLOCK_TOPIC}"),
            LeanGossipTopicKind::Attestation => write!(f, "{LEAN_ATTESTATION_TOPIC}"),
        }
    }
}
