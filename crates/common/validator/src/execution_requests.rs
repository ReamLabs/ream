use alloy_primitives::Bytes;
use ream_consensus::{
    constants::{CONSOLIDATION_REQUEST_TYPE, DEPOSIT_REQUEST_TYPE, WITHDRAWAL_REQUEST_TYPE}, 
    execution_requests::ExecutionRequests,
    consolidation_request::ConsolidationRequest,
    withdrawal_request::WithdrawalRequest,
    deposit_request::DepositRequest
};
use serde_yaml::with;
use ssz_types::VariableList;
use ssz::Decode;
use anyhow::{ Result, anyhow };

fn get_execution_requests(execution_requests_list: Vec<Bytes>) -> Result<ExecutionRequests> {
    let mut deposits: Vec<DepositRequest> = vec![];
    let mut withdrawals: Vec<WithdrawalRequest> = vec![];
    let mut consolidations: Vec<ConsolidationRequest> = vec![];

    let mut prev_req_type: Option<u8> = None;
    for request_bytes in execution_requests_list.into_iter() {
        let request: &[u8] = request_bytes.as_ref();
        if request.len() >= 2 {
            let req_type: u8 = request[0];
            let request_data: &[u8] = &request[1..];

            if let Some(prev_type_unwrapped) = prev_req_type {
                if prev_type_unwrapped >= req_type {
                    return Err(anyhow!("Invalid request type order"));
                }
            }
            prev_req_type = Some(req_type);
            if req_type == DEPOSIT_REQUEST_TYPE[0] {
                match DepositRequest::from_ssz_bytes(request_data) {
                    Ok(deposit) => deposits.push(deposit),
                    Err(e) => return Err(anyhow!("Failed to deserialize DepositRequest: {:?}", e)),
                }
            } else if req_type == WITHDRAWAL_REQUEST_TYPE[0] {
                match WithdrawalRequest::from_ssz_bytes(request_data) {
                    Ok(withdrawal) => withdrawals.push(withdrawal),
                    Err(e) => return Err(anyhow!("Failed to deserialize WithdrawalRequest: {:?}", e)),
                }
            } else if req_type == CONSOLIDATION_REQUEST_TYPE[0] {
                match ConsolidationRequest::from_ssz_bytes(request_data) {
                    Ok(consolidation) => consolidations.push(consolidation),
                    Err(e) => return Err(anyhow!("Failed to deserialize ConsolidationRequest: {:?}", e)),
                }
            }
        } else {
            return Err(anyhow!("Invalid request length"));
        }
    }
    Ok(ExecutionRequests { deposits: VariableList::from(deposits), withdrawals: VariableList::from(withdrawals), consolidations: VariableList::from(consolidations) })
}