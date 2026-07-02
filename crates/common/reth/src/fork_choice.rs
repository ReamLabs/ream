use alloy_primitives::{Address, B256};
use alloy_rpc_types_engine::{ForkchoiceState, ForkchoiceUpdated};
use reth_ethereum::{
    engine::EthPayloadAttributes,
    node::{EthEngineTypes, api::ConsensusEngineHandle},
};
use sha2::{Digest, Sha256};

/// Creates a `ForkchoiceState` used by the Engine API.
///
/// The fork choice state specifies the current head, justified, and finalized
/// block hashes from ream.
pub fn create_fork_choice_state(
    head_block_hash: B256,
    safe_block_hash: B256,
    finalized_block_hash: B256,
) -> ForkchoiceState {
    ForkchoiceState {
        head_block_hash,
        safe_block_hash,
        finalized_block_hash,
    }
}

/// Applies a fork choice update to the execution layer, equivalent to
/// `engine_forkchoiceUpdatedV4`.
///
/// This calls the consensus engine handle directly rather than the JSON-RPC
/// layer, so it is version-agnostic: the effective version is determined by the
/// `payload_attributes` passed. `EthPayloadAttributes` carries the Amsterdam
/// (V4) fields `slot_number` and `target_gas_limit`
pub async fn update(
    consensus_engine_handle: &ConsensusEngineHandle<EthEngineTypes>,
    state: ForkchoiceState,
    payload_attributes: Option<EthPayloadAttributes>,
) -> eyre::Result<ForkchoiceUpdated> {
    let updated = consensus_engine_handle
        .fork_choice_updated(state, payload_attributes)
        .await?;
    Ok(updated)
}

/// Creates an `EthPayloadAttributes` request for payload building.
///
/// These attributes are supplied to the execution layer
/// to initiate the construction of a new payload.
///
/// Withdrawals are currently omitted and set to None.
pub fn create_payload_attributes(
    timestamp: u64,
    prev_randao: B256,
    suggested_fee_recipient: Address,
    parent_beacon_block_root: Option<B256>,
    slot_number: Option<u64>,
    target_gas_limit: Option<u64>,
) -> EthPayloadAttributes {
    EthPayloadAttributes {
        timestamp,
        prev_randao,
        suggested_fee_recipient,
        withdrawals: Some(vec![]),
        parent_beacon_block_root,
        slot_number,
        target_gas_limit,
    }
}

/// Creates payload attributes for a Ream block proposal.
///
/// The timestamp is computed from the slot and genesis time. The
/// `prev_randao` value is deterministically derived from the parent Lean
/// block root and slot.
pub fn create_ream_payload_attributes(
    slot: u64,
    parent_lean_block_root: B256,
    genesis_time: u64,
    seconds_per_slot: u64,
) -> EthPayloadAttributes {
    let prev_randao = {
        // prev_randao = SHA256(parent_lean_block_root || slot)
        let mut h = Sha256::new();
        h.update(parent_lean_block_root.as_slice());
        h.update(slot.to_le_bytes());
        B256::from_slice(&h.finalize())
    };

    create_payload_attributes(
        genesis_time + slot * seconds_per_slot,
        prev_randao,
        Address::ZERO,
        Some(parent_lean_block_root),
        Some(slot),
        None,
    )
}
