use ream_consensus_lean::{block::SignedBlock, vote::SignedValidatorAttestation};

#[derive(Debug, Clone)]
pub enum LeanP2PRequest {
    GossipBlock(SignedBlock),
    GossipVote(SignedValidatorAttestation),
}
