use ream_consensus_misc::{
    beacon_block_header::SignedBeaconBlockHeader, indexed_attestation::IndexedAttestation,
};
use serde::{Deserialize, Serialize};

/// Proposer slashing event.
///
/// The node has received a ProposerSlashing (from P2P or API) that passes
/// validation rules of the `proposer_slashing` topic.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposerSlashingEvent {
    pub signed_header_1: SignedBeaconBlockHeader, // TODO: Properly type this
    pub signed_header_2: SignedBeaconBlockHeader, // TODO: Properly type this
}

/// Attester slashing event.
///
/// The node has received an AttesterSlashing (from P2P or API) that passes
/// validation rules of the `attester_slashing` topic.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttesterSlashingEvent {
    pub attestation_1: IndexedAttestation,
    pub attestation_2: IndexedAttestation,
}
