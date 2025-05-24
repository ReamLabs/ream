use std::collections::HashSet;

use anyhow::{anyhow, ensure};
use ream_bls::{
    PrivateKey,
    signature::BLSSignature,
    traits::{Aggregatable, Signable},
};
use ream_consensus::{
    attestation::Attestation,
    attestation_data::AttestationData,
    constants::{
        DOMAIN_BEACON_ATTESTER, MAX_COMMITTEES_PER_SLOT, MAX_VALIDATORS_PER_COMMITTEE,
        SLOTS_PER_EPOCH,
    },
    electra::beacon_state::BeaconState,
    misc::{compute_signing_root, get_committee_indices},
};
use ssz_types::{
    BitList, BitVector,
    typenum::{U64, U131072},
};

use crate::constants::ATTESTATION_SUBNET_COUNT;

/// Compute the correct subnet for an attestation for Phase 0.
/// Note, this mimics expected future behavior where attestations will be mapped to their shard
/// subnet.
pub fn compute_subnet_for_attestation(
    committees_per_slot: u64,
    slot: u64,
    committee_index: u64,
) -> u64 {
    let slots_since_epoch_start = slot % SLOTS_PER_EPOCH;
    let committee_since_epoch_start = committees_per_slot * slots_since_epoch_start;
    (committee_since_epoch_start + committee_index) % ATTESTATION_SUBNET_COUNT
}

pub fn compute_on_chain_aggregate(mut aggregates: Vec<Attestation>) -> anyhow::Result<Attestation> {
    ensure!(!aggregates.is_empty(), "Attestation list is empty");
    aggregates.sort_by(|a, b| {
        let a_index = get_committee_indices(&a.committee_bits)[0];
        let b_index = get_committee_indices(&b.committee_bits)[0];
        a_index.cmp(&b_index)
    });

    let aggregation_bits_size = (MAX_VALIDATORS_PER_COMMITTEE * MAX_COMMITTEES_PER_SLOT) as usize;
    let mut aggregation_bits = BitList::<U131072>::with_capacity(aggregation_bits_size)
        .map_err(|err| anyhow!("Failed to create BitList for aggregation_bits {err:?}"))?;

    for aggregate in &aggregates {
        for bit in aggregate.aggregation_bits.iter() {
            aggregation_bits
                .set(aggregation_bits.len(), bit)
                .map_err(|err| anyhow!("Failed to set bit: {err:?}"))?;
        }
    }
    let signatures: Vec<&BLSSignature> = aggregates.iter().map(|a| &a.signature).collect();
    let committee_indices = aggregates
        .iter()
        .map(|a: &Attestation| {
            get_committee_indices(&a.committee_bits)
                .into_iter()
                .next()
                .ok_or_else(|| anyhow!("Committee bits must have at least one bit set"))
        })
        .collect::<Result<HashSet<u64>, _>>()?;
    let mut committee_bits = BitVector::<U64>::new();
    for index in 0..MAX_COMMITTEES_PER_SLOT {
        committee_bits
            .set(index as usize, committee_indices.contains(&index))
            .map_err(|err| anyhow!("Failed to set bit {index}: {err:?}"))?;
    }
    Ok(Attestation {
        aggregation_bits,
        data: aggregates[0].data.clone(),
        signature: BLSSignature::aggregate(&signatures)?,
        committee_bits,
    })
}

pub fn get_attestation_signature(
    state: &BeaconState,
    attestation_data: AttestationData,
    private_key: PrivateKey,
) -> anyhow::Result<BLSSignature> {
    let domain = state.get_domain(DOMAIN_BEACON_ATTESTER, Some(attestation_data.target.epoch));
    let signing_root = compute_signing_root(attestation_data, domain);
    Ok(private_key.sign(signing_root.as_ref())?)
}
