use alloy_primitives::{Address, B256};
use alloy_rpc_types_engine::{
    CancunPayloadFields, ExecutionData, ExecutionPayload, ExecutionPayloadEnvelopeV3,
    ExecutionPayloadSidecar, ExecutionPayloadV3, PayloadStatus,
};
use eyre::OptionExt;
use reth_ethereum::{
    engine::EthPayloadAttributes,
    node::{EthEngineTypes, api::ConsensusEngineHandle},
};
use reth_payload_builder::{BuildNewPayload, PayloadBuilderHandle, PayloadKind};

pub struct ElPayloadRequest {
    pub parent_hash: B256,
    pub timestamp: u64,
    pub prev_randao: B256,
    pub fee_recipient: Address,
    pub parent_lean_block_root: B256,
    pub slot_number: u64,
}

/// Builds an execution payload for a Lean block, equivalent to
/// `engine_getPayloadV3`.
///
/// Returns an `ExecutionPayloadEnvelopeV3`, which splits into an
/// `ExecutionPayloadV3` for the Lean block body and a `BlobsBundleV1` for the
/// DA layer.
pub async fn build(
    payload_builder: &PayloadBuilderHandle<EthEngineTypes>,
    request: ElPayloadRequest,
) -> eyre::Result<ExecutionPayloadEnvelopeV3> {
    let attributes = EthPayloadAttributes {
        timestamp: request.timestamp,
        prev_randao: request.prev_randao,
        suggested_fee_recipient: request.fee_recipient,
        withdrawals: Some(Vec::new()), // TODO currently no deposit/withdrawal on Lean
        parent_beacon_block_root: Some(request.parent_lean_block_root),
        slot_number: Some(request.slot_number),
        target_gas_limit: None, // TODO newly introduced in glamsterdam
    };

    let payload_id = payload_builder
        .send_new_payload(BuildNewPayload {
            attributes,
            parent_hash: request.parent_hash,
            cache: None,
            trie_handle: None,
        })
        .await??;

    let built = payload_builder
        .resolve_kind(payload_id, PayloadKind::WaitForPending)
        .await
        .ok_or_eyre("payload could not be resolved")??;

    Ok(built.try_into_v3()?)
}

/// Imports an execution payload from a Lean block into the execution layer via
/// `engine_newPayloadV3`.
///
/// Returns a `PayloadStatus` reporting whether the EL accepted the payload
/// (`Valid`, `Invalid`, or `Syncing`), so the caller must inspect it.
pub async fn import(
    consensus_engine_handle: &ConsensusEngineHandle<EthEngineTypes>,
    payload: ExecutionPayloadV3,
    parent_beacon_block_root: B256,
    versioned_hashes: Vec<B256>,
) -> eyre::Result<PayloadStatus> {
    let execution_data = ExecutionData::new(
        ExecutionPayload::from(payload),
        ExecutionPayloadSidecar::v3(CancunPayloadFields {
            parent_beacon_block_root,
            versioned_hashes,
        }),
    );

    let status = consensus_engine_handle.new_payload(execution_data).await?;

    Ok(status)
}
