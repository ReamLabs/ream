use alloy_rpc_types_engine::{ExecutionData, PayloadStatus};
use eyre::OptionExt;
use reth_ethereum::node::{EthEngineTypes, api::ConsensusEngineHandle};
use reth_payload_builder::{PayloadBuilderHandle, PayloadId, PayloadKind};

/// Builds an execution payload for a Lean block, equivalent to
/// `engine_getPayloadV4`.
pub async fn build(
    payload_builder: &PayloadBuilderHandle<EthEngineTypes>,
    payload_id: PayloadId,
) -> eyre::Result<ExecutionData> {
    // we can Ideally extract out WaitForPending to be more idiomatic
    let built = payload_builder
        .resolve_kind(payload_id, PayloadKind::WaitForPending)
        .await
        .ok_or_eyre("payload could not be resolved")??;

    Ok(built.into_execution_data())
}

/// Imports an execution payload from a Lean block into the execution layer via
/// `engine_newPayloadV4`.
///
/// Returns a `PayloadStatus` reporting whether the EL accepted the payload
/// (`Valid`, `Invalid`, or `Syncing`), so the caller must inspect it.
pub async fn import(
    consensus_engine_handle: &ConsensusEngineHandle<EthEngineTypes>,
    execution_data: ExecutionData,
) -> eyre::Result<PayloadStatus> {
    let status = consensus_engine_handle.new_payload(execution_data).await?;
    Ok(status)
}
