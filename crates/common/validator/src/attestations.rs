use std::collections::HashSet;

use anyhow::anyhow;
use ream_bls::signature::BLSSignature;
use ream_consensus::{
    attestation::Attestation,
    attestation_data::AttestationData,
    constants::{MAX_COMMITTEES_PER_SLOT, MAX_VALIDATORS_PER_COMMITTEE},
    misc::get_committee_indices,
};
use ssz_types::{
    BitList, BitVector,
    typenum::{U64, U131072},
};

pub fn compute_on_chain_aggregate(
    network_aggregates: Vec<Attestation>,
) -> anyhow::Result<Attestation> {
    let mut aggregates: Vec<Attestation> = network_aggregates.clone();

    aggregates.sort_by(|a, b| {
        let a_index = get_committee_indices(&a.committee_bits)[0];
        let b_index = get_committee_indices(&b.committee_bits)[0];
        a_index.cmp(&b_index)
    });

    let data: AttestationData = aggregates[0].data.clone();
    let aggregation_bits_size: usize =
        (MAX_VALIDATORS_PER_COMMITTEE * MAX_COMMITTEES_PER_SLOT) as usize;
    let mut aggregation_bits: BitList<U131072> =
        BitList::<U131072>::with_capacity(aggregation_bits_size)
            .map_err(|_| anyhow::anyhow!("Failed to create BitList for aggregation_bits"))?;

    for a in &aggregates {
        for b in a.aggregation_bits.iter() {
            let bit_index: usize = aggregation_bits.len();
            aggregation_bits
                .set(bit_index, b)
                .map_err(|err| anyhow!("Failed to set bit {bit_index}: {err:?}"))?;
        }
    }
    let signature_list: Vec<BLSSignature> =
        aggregates.iter().map(|a| a.signature.clone()).collect();
    let signature: BLSSignature = BLSSignature::aggregate(signature_list);

    let committee_indices: HashSet<u64> = aggregates
        .iter()
        .map(|a: &Attestation| get_committee_indices(&a.committee_bits)[0])
        .collect();
    let mut committee_bits: BitVector<U64> = BitVector::new();
    for index in 0..MAX_COMMITTEES_PER_SLOT {
        committee_bits
            .set(index as usize, committee_indices.contains(&index))
            .map_err(|err| anyhow!("Failed to set bit {index}: {err:?}"))?;
    }
    Ok(Attestation {
        aggregation_bits,
        data,
        signature,
        committee_bits,
    })
}
