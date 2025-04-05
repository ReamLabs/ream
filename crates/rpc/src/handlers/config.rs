use std::sync::Arc;

use alloy_primitives::Address;
use ream_network_spec::networks::NetworkSpec;
use serde::{Deserialize, Serialize};
use warp::{
    http::status::StatusCode,
    reject::Rejection,
    reply::{Reply, with_status},
};

use super::Data;

#[derive(Serialize, Deserialize, Default)]
struct DepositContract {
    #[serde(with = "serde_utils::quoted_u64")]
    chain_id: u64,
    address: Address,
}

/// Called by `/deposit_contract` to get the Genesis Config of Beacon Chain.
pub async fn get_deposit_contract(network_spec: Arc<NetworkSpec>) -> Result<impl Reply, Rejection> {
    let deposit_contract_response: DepositContract = DepositContract {
        chain_id: network_spec.network.chain_id(),
        address: network_spec.deposit_contract_address,
    };
    Ok(with_status(
        Data::json(deposit_contract_response),
        StatusCode::OK,
    ))
}
