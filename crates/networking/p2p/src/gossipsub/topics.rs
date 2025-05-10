use alloy_primitives::{aliases::B32, hex::ToHexExt};
use libp2p::gossipsub::IdentTopic as Topic;

pub const TOPIC_PREFIX: &str = "eth2";
pub const ENCODING_POSTFIX: &str = "ssz_snappy";
pub const BEACON_BLOCK_TOPIC: &str = "beacon_block";
pub const BEACON_AGGREGATE_AND_PROOF_TOPIC: &str = "beacon_aggregate_and_proof";
pub const VOLUNTARY_EXIT_TOPIC: &str = "voluntary_exit";
pub const PROPOSER_SLASHING_TOPIC: &str = "proposer_slashing";
pub const ATTESTER_SLASHING_TOPIC: &str = "attester_slashing";
pub const BEACON_ATTESTATION_PREFIX: &str = "beacon_attestation_";
pub const SYNC_COMMITTEE_PREFIX_TOPIC: &str = "sync_committee_";
pub const SYNC_COMMITTEE_CONTRIBUTION_AND_PROOF_TOPIC: &str =
    "sync_committee_contribution_and_proof";
pub const BLS_TO_EXECUTION_CHANGE_TOPIC: &str = "bls_to_execution_change";
pub const LIGHT_CLIENT_FINALITY_UPDATE_TOPIC: &str = "light_client_finality_update";
pub const LIGHT_CLIENT_OPTIMISTIC_UPDATE_TOPIC: &str = "light_client_optimistic_update";
pub const BLOB_SIDECAR_PREFIX_TOPIC: &str = "blob_sidecar_";

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub struct GossipTopic {
    pub fork: B32,
    pub kind: GossipTopicKind,
}

impl std::fmt::Display for GossipTopic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "/{}/{}/{}/{}",
            TOPIC_PREFIX,
            self.fork.encode_hex(),
            self.kind,
            ENCODING_POSTFIX
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
    VoluntaryExit,
    ProposerSlashing,
    AttesterSlashing,
    BeaconAttestation(u64),
    SyncCommittee(u64),
    SyncCommitteeContributionAndProof,
    BlsToExecutionChange,
    LightClientFinalityUpdate,
    LightClientOptimisticUpdate,
    BlobSidecar(u64),
}

impl std::fmt::Display for GossipTopicKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GossipTopicKind::BeaconBlock => write!(f, "{BEACON_BLOCK_TOPIC}"),
            GossipTopicKind::BeaconAggregateAndProof => {
                write!(f, "{BEACON_AGGREGATE_AND_PROOF_TOPIC}")
            }
            GossipTopicKind::VoluntaryExit => write!(f, "{VOLUNTARY_EXIT_TOPIC}"),
            GossipTopicKind::ProposerSlashing => write!(f, "{PROPOSER_SLASHING_TOPIC}"),
            GossipTopicKind::AttesterSlashing => write!(f, "{ATTESTER_SLASHING_TOPIC}"),
            GossipTopicKind::BeaconAttestation(subnet_id) => {
                write!(f, "{BEACON_ATTESTATION_PREFIX}{subnet_id}")
            }
            GossipTopicKind::SyncCommittee(sync_subnet_id) => {
                write!(f, "{SYNC_COMMITTEE_PREFIX_TOPIC}{sync_subnet_id}")
            }
            GossipTopicKind::SyncCommitteeContributionAndProof => {
                write!(f, "{SYNC_COMMITTEE_CONTRIBUTION_AND_PROOF_TOPIC}")
            }
            GossipTopicKind::BlsToExecutionChange => {
                write!(f, "{BLS_TO_EXECUTION_CHANGE_TOPIC}")
            }
            GossipTopicKind::LightClientFinalityUpdate => {
                write!(f, "{LIGHT_CLIENT_FINALITY_UPDATE_TOPIC}")
            }
            GossipTopicKind::LightClientOptimisticUpdate => {
                write!(f, "{LIGHT_CLIENT_OPTIMISTIC_UPDATE_TOPIC}")
            }
            GossipTopicKind::BlobSidecar(blob_index) => {
                write!(f, "{BLOB_SIDECAR_PREFIX_TOPIC}{blob_index}")
            }
        }
    }
}
