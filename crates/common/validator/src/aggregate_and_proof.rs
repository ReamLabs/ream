use ream_consensus::attestation::Attestation;
use ream_bls::BLSSignature;

pub struct AggregateAndProof {
    pub aggregator_index: u64,
    pub aggregate: Attestation,
    pub selection_proof: BLSSignature
}