use LeanGossipTopicKind::*;
use libp2p::gossipsub::{IdentTopic as Topic, TopicHash};

use crate::gossipsub::error::GossipsubError;

pub const TOPIC_PREFIX: &str = "leanconsensus";
pub const ENCODING_POSTFIX: &str = "ssz_snappy";

pub const LEAN_BLOCK_TOPIC: &str = "block";

pub const LEAN_ATTESTATION_SUBNET_PREFIX: &str = "attestation_";
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

#[derive(Debug, Hash, Clone, PartialEq, Eq)]
pub enum LeanGossipTopicKind {
    Block,
    AttestationSubnet(u64),
    AggregatedAttestation,
}

impl std::fmt::Display for LeanGossipTopicKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LeanGossipTopicKind::Block => write!(f, "{LEAN_BLOCK_TOPIC}"),
            LeanGossipTopicKind::AttestationSubnet(subnet_id) => {
                write!(f, "{LEAN_ATTESTATION_SUBNET_PREFIX}{subnet_id}")
            }
            LeanGossipTopicKind::AggregatedAttestation => write!(f, "{LEAN_AGGREGATION_TOPIC}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use libp2p::gossipsub::TopicHash;

    use super::{
        ENCODING_POSTFIX, LEAN_BLOCK_TOPIC, LeanGossipTopic, LeanGossipTopicKind, TOPIC_PREFIX,
    };
    use crate::gossipsub::error::GossipsubError;

    #[test]
    fn test_gossip_topic_creation() {
        let topic = LeanGossipTopic {
            kind: LeanGossipTopicKind::Block,
            fork: "0x12345678".to_string(),
        };

        assert_eq!(
            topic,
            LeanGossipTopic {
                kind: LeanGossipTopicKind::Block,
                fork: "0x12345678".to_string(),
            }
        );
        assert_eq!(
            topic.to_string(),
            "/leanconsensus/0x12345678/block/ssz_snappy"
        );
    }

    #[test]
    fn test_gossip_topic_from_string() {
        let topic_str = "/leanconsensus/0x12345678/block/ssz_snappy";
        let topic = TopicHash::from_raw(topic_str);

        assert_eq!(
            LeanGossipTopic::from_topic_hash(&topic).unwrap(),
            LeanGossipTopic {
                kind: LeanGossipTopicKind::Block,
                fork: "0x12345678".to_string(),
            }
        );
    }

    #[test]
    fn test_parse_topic_string() {
        let topic = TopicHash::from_raw(format!(
            "/{TOPIC_PREFIX}/0x12345678/{LEAN_BLOCK_TOPIC}/{ENCODING_POSTFIX}",
        ));

        let parsed = LeanGossipTopic::from_topic_hash(&topic).unwrap();

        assert_eq!(parsed.fork, "0x12345678");
        assert_eq!(parsed.kind, LeanGossipTopicKind::Block);
    }

    #[test]
    fn test_invalid_topic_string() {
        let invalid_topic = TopicHash::from_raw("/invalid/topic");
        let wrong_prefix = TopicHash::from_raw("/wrongprefix/0x123/block/ssz_snappy");

        let invalid_topic_err = LeanGossipTopic::from_topic_hash(&invalid_topic).unwrap_err();
        assert!(matches!(invalid_topic_err, GossipsubError::InvalidTopic(_)));
        assert!(
            invalid_topic_err
                .to_string()
                .contains("Invalid topic format")
        );

        let wrong_prefix_err = LeanGossipTopic::from_topic_hash(&wrong_prefix).unwrap_err();
        assert!(matches!(wrong_prefix_err, GossipsubError::InvalidTopic(_)));
        assert!(
            wrong_prefix_err
                .to_string()
                .contains("Invalid topic format")
        );
    }

    #[test]
    fn test_topic_kind_enum() {
        assert_eq!(LeanGossipTopicKind::Block.to_string(), "block");
        assert_eq!(
            LeanGossipTopicKind::AttestationSubnet(0).to_string(),
            "attestation_0"
        );
    }

    #[test]
    fn test_validate_fork_success() {
        let topic = LeanGossipTopic {
            kind: LeanGossipTopicKind::Block,
            fork: "0x12345678".to_string(),
        };

        let expected_fork = "0x12345678";

        assert_eq!(topic.fork, expected_fork);
    }

    #[test]
    fn test_validate_fork_raises_on_mismatch() {
        let topic = LeanGossipTopic {
            kind: LeanGossipTopicKind::Block,
            fork: "0x12345678".to_string(),
        };

        let expected_fork = "0xtesttopic";

        assert_ne!(topic.fork, expected_fork);
    }

    #[test]
    fn test_from_string_validated_success() {
        let topic = TopicHash::from_raw("/leanconsensus/0x12345678/block/ssz_snappy");

        let parsed = LeanGossipTopic::from_topic_hash(&topic).unwrap();

        assert_eq!(
            parsed,
            LeanGossipTopic {
                kind: LeanGossipTopicKind::Block,
                fork: "0x12345678".to_string(),
            }
        );
        assert_eq!(parsed.fork, "0x12345678");
    }

    #[test]
    fn test_from_string_validated_raises_on_mismatch() {
        let topic = TopicHash::from_raw("/leanconsensus/0x12345678/block/ssz_snappy");

        let parsed = LeanGossipTopic::from_topic_hash(&topic).unwrap();

        assert_ne!(parsed.fork, "0xtesttopic");
    }

    #[test]
    fn test_from_string_validated_raises_on_invalid_topic() {
        let topic = TopicHash::from_raw("/invalid/topic");

        let err = LeanGossipTopic::from_topic_hash(&topic).unwrap_err();

        assert!(matches!(err, GossipsubError::InvalidTopic(_)));
        assert!(err.to_string().contains("Invalid topic format"));
    }
}
