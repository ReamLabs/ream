pub const BEACON_BLOCK_TOPIC: &str = "beacon_block";
pub const BEACON_AGGREGATE_AND_PROOF_TOPIC: &str = "beacon_aggregate_and_proof";
pub const BEACON_ATTESTATION_PREFIX: &str = "beacon_attestation_";
pub const VOLUNTARY_EXIT_TOPIC: &str = "voluntary_exit";
pub const PROPOSER_SLASHING_TOPIC: &str = "proposer_slashing";
pub const ATTESTER_SLASHING_TOPIC: &str = "attester_slashing";

#[derive(Debug, Hash, Clone, Copy, PartialEq, Eq)]
pub enum GossipTopic {
    BeaconBlock,
    BeaconAggregateAndProof,
    BeaconAttestation(u64),
    VoluntaryExit,
    ProposerSlashing,
    AttesterSlashing,
}

#[derive(Debug)]
pub struct TopicName {
    pub name: String,
}
