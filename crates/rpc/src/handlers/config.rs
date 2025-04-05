use alloy_primitives::Address;
use serde::{Deserialize, Serialize};
use warp::{
    http::status::StatusCode,
    reject::Rejection,
    reply::{Reply, with_status},
};

use super::Data;

#[derive(Serialize, Deserialize, Default)]
struct DepositContract {
    chain_id: u64,
    address: Address
}

/// Called by `/deposit_contract` to get the Genesis Config of Beacon Chain.
pub async fn get_deposit_contract(chain_id: u64, deposit_contract_address: Address) -> Result<impl Reply, Rejection> {
    let deposit_contract_response: DepositContract = DepositContract {
        chain_id,
        address: deposit_contract_address
    };
    Ok(with_status(Data::json(deposit_contract_response), StatusCode::OK))
}
