use libp2p::gossipsub::TopicHash;
#[cfg(feature = "devnet4")]
use ream_consensus_lean::{
    attestation::{SignedAggregatedAttestation, SignedAttestation},
    block::SignedBlock,
};
#[cfg(feature = "devnet3")]
use ream_consensus_lean::{
    attestation::{SignedAggregatedAttestation, SignedAttestation},
    block::SignedBlockWithAttestation,
};
use ssz::Decode;

use super::topics::{LeanGossipTopic, LeanGossipTopicKind};
use crate::gossipsub::error::GossipsubError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LeanGossipsubMessage {
    #[cfg(feature = "devnet3")]
    Block(Box<SignedBlockWithAttestation>),
    #[cfg(feature = "devnet4")]
    Block(Box<SignedBlock>),
    Attestation {
        subnet_id: u64,
        attestation: Box<SignedAttestation>,
    },
    AggregatedAttestation(Box<SignedAggregatedAttestation>),
}

impl LeanGossipsubMessage {
    pub fn decode(topic: &TopicHash, data: &[u8]) -> Result<Self, GossipsubError> {
        match LeanGossipTopic::from_topic_hash(topic)?.kind {
            #[cfg(feature = "devnet3")]
            LeanGossipTopicKind::Block => Ok(Self::Block(Box::new(
                SignedBlockWithAttestation::from_ssz_bytes(data)?,
            ))),
            #[cfg(feature = "devnet4")]
            LeanGossipTopicKind::Block => {
                Ok(Self::Block(Box::new(SignedBlock::from_ssz_bytes(data)?)))
            }
            LeanGossipTopicKind::AttestationSubnet(subnet_id) => Ok(Self::Attestation {
                subnet_id,
                attestation: Box::new(SignedAttestation::from_ssz_bytes(data)?),
            }),
            LeanGossipTopicKind::AggregatedAttestation => Ok(Self::AggregatedAttestation(
                Box::new(SignedAggregatedAttestation::from_ssz_bytes(data)?),
            )),
        }
    }
}
