use alloy_primitives::{Address, B256};
use reth_ethereum::engine::EthPayloadAttributes;
use sha2::{Digest, Sha256};

/// Builds the execution payload attributes for a lean block proposal at `slot`.
///
/// The timestamp is derived from the slot and the genesis time, and `prev_randao` from
/// `SHA256(parent_lean_block_root || slot)`. Nothing on the lean chain validates `prev_randao`, so
/// this is only a deterministic way for the proposer to fill a field the EL requires.
pub fn build_payload_attributes(
    slot: u64,
    parent_lean_block_root: B256,
    genesis_time: u64,
    seconds_per_slot: u64,
) -> EthPayloadAttributes {
    let mut hasher = Sha256::new();
    hasher.update(parent_lean_block_root.as_slice());
    hasher.update(slot.to_le_bytes());

    EthPayloadAttributes {
        timestamp: genesis_time + slot * seconds_per_slot,
        prev_randao: B256::from_slice(&hasher.finalize()),
        suggested_fee_recipient: Address::ZERO,
        withdrawals: Some(vec![]),
        parent_beacon_block_root: Some(parent_lean_block_root),
        slot_number: None,
        target_gas_limit: None,
    }
}
