use ream_consensus_lean::{attestation::SignedAttestation, block::SignedBlock};

#[derive(Debug, Clone)]
pub enum LeanP2PRequest {
    GossipBlock(SignedBlock),
    GossipAttestation(SignedAttestation),
}
