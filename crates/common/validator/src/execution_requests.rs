use alloy_primitives::Bytes;
use anyhow::{Result, anyhow, bail, ensure};
use ream_consensus::{
    consolidation_request::ConsolidationRequest,
    constants::{CONSOLIDATION_REQUEST_TYPE, DEPOSIT_REQUEST_TYPE, WITHDRAWAL_REQUEST_TYPE},
    deposit_request::DepositRequest,
    execution_requests::ExecutionRequests,
    withdrawal_request::WithdrawalRequest,
};
use ssz::Decode;
use ssz_types::{
    VariableList,
    typenum::{U2, U16, U8192},
};

pub fn get_execution_requests(execution_requests_list: Vec<Bytes>) -> Result<ExecutionRequests> {
    let mut deposits = None;
    let mut withdrawals = None;
    let mut consolidations = None;
    let mut previous_request_type: Option<u8> = None;
    for request_bytes in execution_requests_list.into_iter() {
        ensure!(request_bytes.len() >= 2, "Invalid request length");
        let request_type = request_bytes[0];
        ensure!(
            previous_request_type.is_none() || previous_request_type < Some(request_type),
            "Duplicate request type found or list wasn't in strictly ascending order  in execution requests"
        );
        previous_request_type = Some(request_type);
        match request_type {
            DEPOSIT_REQUEST_TYPE => {
                ensure!(
                    deposits.is_none(),
                    "Multiple deposit requests found in execution requests"
                );
                deposits = Some(
                    VariableList::<DepositRequest, U8192>::from_ssz_bytes(&request_bytes[1..])
                        .map_err(|err| anyhow!("Failed to deserialize DepositRequest: {err:?}"))?,
                );
            }
            WITHDRAWAL_REQUEST_TYPE => {
                ensure!(
                    withdrawals.is_none(),
                    "Multiple withdrawal requests found in execution requests"
                );
                withdrawals = Some(
                    VariableList::<WithdrawalRequest, U16>::from_ssz_bytes(&request_bytes[1..])
                        .map_err(|err| {
                            anyhow!("Failed to deserialize WithdrawalRequest: {err:?}")
                        })?,
                );
            }
            CONSOLIDATION_REQUEST_TYPE => {
                ensure!(
                    consolidations.is_none(),
                    "Multiple consolidation requests found in execution requests"
                );
                consolidations = Some(
                    VariableList::<ConsolidationRequest, U2>::from_ssz_bytes(&request_bytes[1..])
                        .map_err(|err| {
                        anyhow!("Failed to deserialize ConsolidationRequest: {err:?}")
                    })?,
                );
            }
            _ => {
                bail!("Invalid request type: {request_type}");
            }
        }
    }
    Ok(ExecutionRequests {
        deposits: deposits.unwrap_or_default(),
        withdrawals: withdrawals.unwrap_or_default(),
        consolidations: consolidations.unwrap_or_default(),
    })
}
