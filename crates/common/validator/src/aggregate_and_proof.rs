use ream_bls::{BLSSignature, PrivateKey, traits::Signable};
use ream_consensus::{
    attestation::Attestation,
    constants::DOMAIN_AGGREGATE_AND_PROOF,
    electra::beacon_state::BeaconState,
    misc::{compute_epoch_at_slot, compute_signing_root},
};
use serde::{Deserialize, Serialize};
use ssz_derive::{Decode, Encode};
use tree_hash_derive::TreeHash;

use crate::attestation::get_slot_signature;

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize, Encode, Decode, TreeHash)]
pub struct AggregateAndProof {
    pub aggregator_index: u64,
    pub aggregate: Attestation,
    pub selection_proof: BLSSignature,
}

pub fn get_aggregate_and_proof(
    state: &BeaconState,
    aggregator_index: u64,
    aggregate: Attestation,
    private_key: PrivateKey,
) -> anyhow::Result<AggregateAndProof> {
    Ok(AggregateAndProof {
        selection_proof: get_slot_signature(state, aggregate.data.slot, private_key)?,
        aggregator_index,
        aggregate,
    })
}

pub fn get_aggregate_and_proof_signature(
    state: &BeaconState,
    aggregate_and_proof: AggregateAndProof,
    private_key: PrivateKey,
) -> anyhow::Result<BLSSignature> {
    let domain = state.get_domain(
        DOMAIN_AGGREGATE_AND_PROOF,
        Some(compute_epoch_at_slot(
            aggregate_and_proof.aggregate.data.slot,
        )),
    );
    let signing_root = compute_signing_root(aggregate_and_proof, domain);
    Ok(private_key.sign(signing_root.as_ref())?)
}
