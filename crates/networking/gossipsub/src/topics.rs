use libp2p::gossipsub::IdentTopic as Topic;

pub const TOPIC_PREFIX: &str = "eth2";
pub const ENCODING_POSTFIX: &str = "ssz_snappy";
pub const BEACON_BLOCK_TOPIC: &str = "beacon_block";
pub const BEACON_AGGREGATE_AND_PROOF_TOPIC: &str = "beacon_aggregate_and_proof";
pub const BEACON_ATTESTATION_PREFIX: &str = "beacon_attestation_";
pub const VOLUNTARY_EXIT_TOPIC: &str = "voluntary_exit";
pub const PROPOSER_SLASHING_TOPIC: &str = "proposer_slashing";
pub const ATTESTER_SLASHING_TOPIC: &str = "attester_slashing";

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub struct GossipTopic {
    pub fork: [u8; 4],
    pub kind: GossipTopicKind,
}

impl std::fmt::Display for GossipTopic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "/{}/{}/{}/{}",
            TOPIC_PREFIX,
            hex::encode(self.fork),
            ENCODING_POSTFIX,
            self.kind
        )
    }
}

impl From<GossipTopic> for Topic {
    fn from(topic: GossipTopic) -> Topic {
        Topic::new(topic)
    }
}

impl From<GossipTopic> for String {
    fn from(topic: GossipTopic) -> Self {
        topic.to_string()
    }
}

#[derive(Debug, Hash, Clone, Copy, PartialEq, Eq)]
pub enum GossipTopicKind {
    BeaconBlock,
    BeaconAggregateAndProof,
    BeaconAttestation(u64),
    VoluntaryExit,
    ProposerSlashing,
    AttesterSlashing,
}

impl std::fmt::Display for GossipTopicKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GossipTopicKind::BeaconBlock => write!(f, "{}", BEACON_BLOCK_TOPIC),
            GossipTopicKind::BeaconAggregateAndProof => {
                write!(f, "{}", BEACON_AGGREGATE_AND_PROOF_TOPIC)
            }
            GossipTopicKind::BeaconAttestation(slot) => {
                write!(f, "{}{}", BEACON_ATTESTATION_PREFIX, slot)
            }
            GossipTopicKind::VoluntaryExit => write!(f, "{}", VOLUNTARY_EXIT_TOPIC),
            GossipTopicKind::ProposerSlashing => write!(f, "{}", PROPOSER_SLASHING_TOPIC),
            GossipTopicKind::AttesterSlashing => write!(f, "{}", ATTESTER_SLASHING_TOPIC),
        }
    }
}
