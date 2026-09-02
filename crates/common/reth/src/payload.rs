use alloy_eips::eip4895::Withdrawal as AlloyWithdrawal;
use alloy_primitives::{B256, Bloom, Bytes};
use alloy_rpc_types_engine::{
    CancunPayloadFields, ExecutionData, ExecutionPayload, ExecutionPayloadSidecar,
    ExecutionPayloadV1, ExecutionPayloadV2, ExecutionPayloadV3, PraguePayloadFields,
};
use anyhow::{anyhow, bail};
use ream_consensus_misc::withdrawal::Withdrawal;
use ream_execution_rpc_types::electra::execution_payload::ExecutionPayload as ReamExecutionPayload;
use ssz_types::{FixedVector, VariableList};

/// Convert reth's alloy `ExecutionData` into ream's `ExecutionPayload`.
///
/// The EL runs Prague, whose `ExecutionPayload` container is still the
/// V3 shape — Prague's EIP-7685 execution requests are carried in the
/// `newPayloadV4` sidecar as a separate parameter, not inside the payload body.
/// So the payload is always an `ExecutionPayload::V3` here
pub fn to_ream_execution_payload(data: &ExecutionData) -> anyhow::Result<ReamExecutionPayload> {
    let ExecutionPayload::V3(v3) = &data.payload else {
        bail!("expected a V3 execution payload body (Cancun or later active in the EL genesis)");
    };
    let v1 = &v3.payload_inner.payload_inner;

    let transactions = v1
        .transactions
        .iter()
        .map(|tx| {
            VariableList::new(tx.to_vec()).map_err(|err| anyhow!("tx exceeds SSZ limit: {err:?}"))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;

    let withdrawals = v3
        .payload_inner
        .withdrawals
        .iter()
        .map(|withdrawal| Withdrawal {
            index: withdrawal.index,
            validator_index: withdrawal.validator_index,
            address: withdrawal.address,
            amount: withdrawal.amount,
        })
        .collect::<Vec<_>>();

    Ok(ReamExecutionPayload {
        parent_hash: v1.parent_hash,
        fee_recipient: v1.fee_recipient,
        state_root: v1.state_root,
        receipts_root: v1.receipts_root,
        logs_bloom: FixedVector::new(v1.logs_bloom.as_slice().to_vec())
            .map_err(|err| anyhow!("logs bloom must be 256 bytes: {err:?}"))?,
        prev_randao: v1.prev_randao,
        block_number: v1.block_number,
        gas_limit: v1.gas_limit,
        gas_used: v1.gas_used,
        timestamp: v1.timestamp,
        extra_data: VariableList::new(v1.extra_data.to_vec())
            .map_err(|err| anyhow!("extra data exceeds 32 bytes: {err:?}"))?,
        base_fee_per_gas: v1.base_fee_per_gas,
        block_hash: v1.block_hash,
        transactions: VariableList::new(transactions)
            .map_err(|err| anyhow!("too many transactions for SSZ list: {err:?}"))?,
        withdrawals: VariableList::new(withdrawals)
            .map_err(|err| anyhow!("too many withdrawals for SSZ list: {err:?}"))?,
        blob_gas_used: v3.blob_gas_used,
        excess_blob_gas: v3.excess_blob_gas,
    })
}

/// Rebuild reth's alloy `ExecutionData` from a ream `ExecutionPayload` so it can
/// be re-imported via `engine_newPayloadV4` — the inverse of [`to_ream_execution_payload`].
pub fn from_ream_execution_payload(
    payload: &ReamExecutionPayload,
    parent_root: B256,
) -> ExecutionData {
    let transactions = payload
        .transactions
        .iter()
        .map(|tx| Bytes::from(tx.to_vec()))
        .collect::<Vec<_>>();

    let withdrawals = payload
        .withdrawals
        .iter()
        .map(|withdrawal| AlloyWithdrawal {
            index: withdrawal.index,
            validator_index: withdrawal.validator_index,
            address: withdrawal.address,
            amount: withdrawal.amount,
        })
        .collect::<Vec<_>>();

    let v1 = ExecutionPayloadV1 {
        parent_hash: payload.parent_hash,
        fee_recipient: payload.fee_recipient,
        state_root: payload.state_root,
        receipts_root: payload.receipts_root,
        logs_bloom: Bloom::from_slice(payload.logs_bloom.as_ref()),
        prev_randao: payload.prev_randao,
        block_number: payload.block_number,
        gas_limit: payload.gas_limit,
        gas_used: payload.gas_used,
        timestamp: payload.timestamp,
        extra_data: Bytes::from(payload.extra_data.to_vec()),
        base_fee_per_gas: payload.base_fee_per_gas,
        block_hash: payload.block_hash,
        transactions,
    };
    let v2 = ExecutionPayloadV2 {
        payload_inner: v1,
        withdrawals,
    };
    let v3 = ExecutionPayloadV3 {
        payload_inner: v2,
        blob_gas_used: payload.blob_gas_used,
        excess_blob_gas: payload.excess_blob_gas,
    };

    let sidecar = ExecutionPayloadSidecar::v4(
        CancunPayloadFields {
            parent_beacon_block_root: parent_root,
            versioned_hashes: Vec::new(),
        },
        PraguePayloadFields::new(Vec::<Bytes>::new()),
    );

    ExecutionData::new(ExecutionPayload::V3(v3), sidecar)
}
