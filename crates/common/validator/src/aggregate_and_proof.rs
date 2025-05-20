use ream_bls::BLSSignature;
use ream_consensus::attestation::Attestation;

pub struct AggregateAndProof {
    pub aggregator_index: u64,
    pub aggregate: Attestation,
    pub selection_proof: BLSSignature,
}
