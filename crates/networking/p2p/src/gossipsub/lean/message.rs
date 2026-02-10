use libp2p::gossipsub::TopicHash;
#[cfg(feature = "devnet3")]
use ream_consensus_lean::attestation::SignedAggregatedAttestation;
use ream_consensus_lean::{attestation::SignedAttestation, block::SignedBlockWithAttestation};
use ssz::Decode;

use super::topics::{LeanGossipTopic, LeanGossipTopicKind};
use crate::gossipsub::error::GossipsubError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LeanGossipsubMessage {
    Block(Box<SignedBlockWithAttestation>),
    #[cfg(feature = "devnet2")]
    Attestation(Box<SignedAttestation>),
    #[cfg(feature = "devnet3")]
    Attestation {
        subnet_id: u64,
        attestation: Box<SignedAttestation>,
    },
    #[cfg(feature = "devnet3")]
    AggregatedAttestation(Box<SignedAggregatedAttestation>),
}

impl LeanGossipsubMessage {
    pub fn decode(topic: &TopicHash, data: &[u8]) -> Result<Self, GossipsubError> {
        match LeanGossipTopic::from_topic_hash(topic)?.kind {
            LeanGossipTopicKind::Block => Ok(Self::Block(Box::new(
                SignedBlockWithAttestation::from_ssz_bytes(data)?,
            ))),
            #[cfg(feature = "devnet2")]
            LeanGossipTopicKind::Attestation => Ok(Self::Attestation(Box::new(
                SignedAttestation::from_ssz_bytes(data)?,
            ))),
            #[cfg(feature = "devnet3")]
            LeanGossipTopicKind::AttestationSubnet(subnet_id) => Ok(Self::Attestation {
                subnet_id,
                attestation: Box::new(SignedAttestation::from_ssz_bytes(data)?),
            }),
            #[cfg(feature = "devnet3")]
            LeanGossipTopicKind::AggregatedAttestation => Ok(Self::AggregatedAttestation(
                Box::new(SignedAggregatedAttestation::from_ssz_bytes(data)?),
            )),
        }
    }
}
