use LeanGossipTopicKind::*;
use libp2p::gossipsub::{IdentTopic as Topic, TopicHash};

use crate::gossipsub::error::GossipsubError;

pub const TOPIC_PREFIX: &str = "leanconsensus";
pub const ENCODING_POSTFIX: &str = "ssz_snappy";

#[cfg(feature = "devnet2")]
pub const LEAN_BLOCK_TOPIC: &str = "block";
#[cfg(feature = "devnet3")]
pub const LEAN_BLOCK_TOPIC: &str = "blocks";

#[cfg(feature = "devnet2")]
pub const LEAN_ATTESTATION_TOPIC: &str = "attestation";

#[cfg(feature = "devnet3")]
pub const LEAN_ATTESTATION_SUBNET_PREFIX: &str = "attestation_";
#[cfg(feature = "devnet3")]
pub const LEAN_AGGREGATION_TOPIC: &str = "aggregation";

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
        let topic_name = topic_parts[2];

        #[cfg(feature = "devnet2")]
        let kind = match topic_name {
            LEAN_BLOCK_TOPIC => LeanGossipTopicKind::Block,
            LEAN_ATTESTATION_TOPIC => LeanGossipTopicKind::Attestation,
            other => {
                return Err(GossipsubError::InvalidTopic(format!(
                    "Invalid topic: {other:?}"
                )));
            }
        };

        #[cfg(feature = "devnet3")]
        let kind = if topic_name == LEAN_BLOCK_TOPIC {
            LeanGossipTopicKind::Block
        } else if topic_name == LEAN_AGGREGATION_TOPIC {
            LeanGossipTopicKind::AggregatedAttestation
        } else if let Some(subnet_str) = topic_name.strip_prefix(LEAN_ATTESTATION_SUBNET_PREFIX) {
            let subnet_id = subnet_str.parse::<u64>().map_err(|err| {
                GossipsubError::InvalidTopic(format!(
                    "Invalid attestation subnet id: {subnet_str:?}, error: {err}"
                ))
            })?;
            LeanGossipTopicKind::AttestationSubnet(subnet_id)
        } else {
            return Err(GossipsubError::InvalidTopic(format!(
                "Invalid topic: {topic_name:?}"
            )));
        };

        Ok(LeanGossipTopic { fork, kind })
    }
}

impl std::fmt::Display for LeanGossipTopic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        #[cfg(feature = "devnet2")]
        let topic_name = match &self.kind {
            Block => LEAN_BLOCK_TOPIC.to_string(),
            Attestation => LEAN_ATTESTATION_TOPIC.to_string(),
        };

        #[cfg(feature = "devnet3")]
        let topic_name = match &self.kind {
            Block => LEAN_BLOCK_TOPIC.to_string(),
            AttestationSubnet(subnet_id) => format!("{LEAN_ATTESTATION_SUBNET_PREFIX}{subnet_id}"),
            AggregatedAttestation => LEAN_AGGREGATION_TOPIC.to_string(),
        };

        write!(
            f,
            "/{TOPIC_PREFIX}/{}/{topic_name}/{ENCODING_POSTFIX}",
            self.fork,
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
        #[cfg(feature = "devnet2")]
        let kind_str = match &val.kind {
            Block => LEAN_BLOCK_TOPIC,
            Attestation => LEAN_ATTESTATION_TOPIC,
        };

        #[cfg(feature = "devnet3")]
        let kind_str = match &val.kind {
            Block => LEAN_BLOCK_TOPIC.to_string(),
            AttestationSubnet(subnet_id) => format!("{LEAN_ATTESTATION_SUBNET_PREFIX}{subnet_id}"),
            AggregatedAttestation => LEAN_AGGREGATION_TOPIC.to_string(),
        };

        TopicHash::from_raw(format!(
            "/{TOPIC_PREFIX}/{}/{kind_str}/{ENCODING_POSTFIX}",
            val.fork,
        ))
    }
}

#[cfg(feature = "devnet2")]
#[derive(Debug, Hash, Clone, Copy, PartialEq, Eq)]
pub enum LeanGossipTopicKind {
    Block,
    Attestation,
}

#[cfg(feature = "devnet3")]
#[derive(Debug, Hash, Clone, PartialEq, Eq)]
pub enum LeanGossipTopicKind {
    Block,
    AttestationSubnet(u64),
    AggregatedAttestation,
}

impl std::fmt::Display for LeanGossipTopicKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        #[cfg(feature = "devnet2")]
        match self {
            LeanGossipTopicKind::Block => write!(f, "{LEAN_BLOCK_TOPIC}"),
            LeanGossipTopicKind::Attestation => write!(f, "{LEAN_ATTESTATION_TOPIC}"),
        }

        #[cfg(feature = "devnet3")]
        match self {
            LeanGossipTopicKind::Block => write!(f, "{LEAN_BLOCK_TOPIC}"),
            LeanGossipTopicKind::AttestationSubnet(subnet_id) => {
                write!(f, "{LEAN_ATTESTATION_SUBNET_PREFIX}{subnet_id}")
            }
            LeanGossipTopicKind::AggregatedAttestation => write!(f, "{LEAN_AGGREGATION_TOPIC}"),
        }
    }
}
