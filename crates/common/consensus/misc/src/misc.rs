use std::cmp::max;

use alloy_primitives::{B256, aliases::B32};
use anyhow::ensure;
use ethereum_hashing::hash;
use ssz_types::{BitVector, typenum::U64};
use tree_hash::TreeHash;

use crate::{
    constants::{
        COMPOUNDING_WITHDRAWAL_PREFIX, EPOCHS_PER_SYNC_COMMITTEE_PERIOD, GENESIS_FORK_VERSION,
        MAX_SEED_LOOKAHEAD, SHUFFLE_ROUND_COUNT, SLOTS_PER_EPOCH,
    },
    fork_data::ForkData,
    signing_data::SigningData,
};

pub mod checksummed_address {
    use alloy_primitives::Address;
    use serde::{Deserialize, Deserializer, Serializer, de::Error as _};

    pub fn serialize<S>(address: &Address, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let checksummed = address.to_checksum(None);
        serializer.serialize_str(&checksummed)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Address, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s: String = Deserialize::deserialize(deserializer)?;
        s.parse::<Address>().map_err(D::Error::custom)
    }
}

pub fn compute_signing_root<SSZObject: TreeHash>(ssz_object: SSZObject, domain: B256) -> B256 {
    SigningData {
        object_root: ssz_object.tree_hash_root(),
        domain,
    }
    .tree_hash_root()
}

pub fn compute_shuffled_index(
    mut index: usize,
    index_count: usize,
    seed: B256,
) -> anyhow::Result<usize> {
    ensure!(index < index_count, "Index must be less than index_count");
    for round in 0..SHUFFLE_ROUND_COUNT {
        let seed_with_round = [seed.as_slice(), &round.to_le_bytes()].concat();
        let pivot = bytes_to_int64(&hash(&seed_with_round)[..]) % index_count as u64;

        let flip = (pivot as usize + (index_count - index)) % index_count;
        let position = max(index, flip);
        let seed_with_position = [
            seed_with_round.as_slice(),
            &(position / 256).to_le_bytes()[0..4],
        ]
        .concat();
        let source = hash(&seed_with_position);
        let byte = source[(position % 256) / 8];
        let bit = (byte >> (position % 8)) % 2;

        index = if bit == 1 { flip } else { index };
    }
    Ok(index)
}

// Return the integer deserialization of ``data`` interpreted as ``ENDIANNESS``-endian.
pub fn bytes_to_int64(slice: &[u8]) -> u64 {
    let mut bytes = [0u8; 8];
    let len = slice.len().min(8);
    bytes[..len].copy_from_slice(&slice[..len]);
    u64::from_le_bytes(bytes)
}

/// Return the committee corresponding to ``indices``, ``seed``, ``index``, and committee ``count``.
pub fn compute_committee(
    indices: &[u64],
    seed: B256,
    index: u64,
    count: u64,
) -> anyhow::Result<Vec<u64>> {
    let start = (indices.len() as u64 * index) / count;
    let end = (indices.len() as u64 * (index + 1)) / count;
    (start..end)
        .map(|i| {
            let shuffled_index = compute_shuffled_index(i as usize, indices.len(), seed)?;
            indices
                .get(shuffled_index)
                .copied()
                .ok_or_else(|| anyhow::anyhow!("Index out of bounds: {}", shuffled_index))
        })
        .collect::<anyhow::Result<Vec<u64>>>()
}

pub fn is_shuffling_stable(slot: u64) -> bool {
    slot % SLOTS_PER_EPOCH != 0
}

/// Return the epoch number at ``slot``.
pub fn compute_epoch_at_slot(slot: u64) -> u64 {
    slot / SLOTS_PER_EPOCH
}

/// Return the start slot of ``epoch``.
pub fn compute_start_slot_at_epoch(epoch: u64) -> u64 {
    epoch * SLOTS_PER_EPOCH
}

/// Return the epoch during which validator activations and exits initiated in ``epoch`` take
/// effect.
pub fn compute_activation_exit_epoch(epoch: u64) -> u64 {
    epoch + 1 + MAX_SEED_LOOKAHEAD
}

/// Return the domain for the ``domain_type`` and ``fork_version``
pub fn compute_domain(
    domain_type: B32,
    fork_version: Option<B32>,
    genesis_validators_root: Option<B256>,
) -> B256 {
    let fork_data = ForkData {
        current_version: fork_version.unwrap_or(GENESIS_FORK_VERSION), /* Fork version for
                                                                        * Ethereum mainnet */
        genesis_validators_root: genesis_validators_root.unwrap_or_default(),
    };
    let fork_data_root = fork_data.compute_fork_data_root();
    let domain_bytes = [&domain_type.0, &fork_data_root.0[..28]].concat();
    B256::from_slice(&domain_bytes)
}

pub fn is_sorted_and_unique(indices: &[usize]) -> bool {
    indices.windows(2).all(|w| w[0] < w[1])
}

pub fn is_compounding_withdrawal_credential(withdrawal_credentials: B256) -> bool {
    &withdrawal_credentials[..1] == COMPOUNDING_WITHDRAWAL_PREFIX
}

pub fn get_committee_indices(commitee_bits: &BitVector<U64>) -> Vec<u64> {
    commitee_bits
        .iter()
        .enumerate()
        .filter_map(|(i, bit)| bit.then_some(i as u64))
        .collect()
}

pub fn compute_sync_committee_period(epoch: u64) -> u64 {
    epoch / EPOCHS_PER_SYNC_COMMITTEE_PERIOD
}

pub fn compute_sync_committee_period_at_slot(slot: u64) -> u64 {
    compute_sync_committee_period(compute_epoch_at_slot(slot))
}
