use ream_consensus_lean::{attestation::SignedAttestation, block::SignedBlockWithAttestation};

#[derive(Debug, Clone)]
pub enum LeanP2PRequest {
    GossipBlock(Box<SignedBlockWithAttestation>),
    GossipAttestation(Box<SignedAttestation>),
}
