use std::sync::Arc;

use alloy_primitives::Address;
use ream_consensus::constants::{DOMAIN_AGGREGATE_AND_PROOF, INACTIVITY_PENALTY_QUOTIENT_BELLATRIX};
use ream_network_spec::networks::NetworkSpec;
use serde::{Deserialize, Serialize};
use warp::{
    http::status::StatusCode,
    reject::Rejection,
    reply::{Reply, with_status},
};

use super::Data;

#[derive(Serialize, Deserialize, Default)]
pub struct DepositContract {
    #[serde(with = "serde_utils::quoted_u64")]
    chain_id: u64,
    address: Address,
}
#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub struct SpecConfig {
    deposit_contract_address: String,
    #[serde(with = "serde_utils::quoted_u64")]
    deposit_network_id: u64,
    domain_aggregate_and_proof: String,
    #[serde(with = "serde_utils::quoted_u64")]
    inactivity_penalty_quotient: u64,
}

impl DepositContract {
    pub fn new(chain_id: u64, address: Address) -> Self {
        Self { chain_id, address }
    }
}
impl SpecConfig {
    pub fn new(
        deposit_contract_address: String,
        deposit_network_id: u64,
        domain_aggregate_and_proof: String,
        inactivity_penalty_quotient: u64,
    ) -> Self {
        Self {
            deposit_contract_address,
            deposit_network_id,
            domain_aggregate_and_proof,
            inactivity_penalty_quotient,
        }
    }

    pub fn from_network_spec(network_spec: &NetworkSpec) -> Self {
        Self::new(
            network_spec.deposit_contract_address.to_string(),
            network_spec.network.chain_id(),
            DOMAIN_AGGREGATE_AND_PROOF.to_string(),
            INACTIVITY_PENALTY_QUOTIENT_BELLATRIX,
        )
    }
}

/// Called by `/deposit_contract` to get the Genesis Config of Beacon Chain.
pub async fn get_deposit_contract(network_spec: Arc<NetworkSpec>) -> Result<impl Reply, Rejection> {
    Ok(with_status(
        Data::json(DepositContract::new(
            network_spec.network.chain_id(),
            network_spec.deposit_contract_address,
        )),
        StatusCode::OK,
    ))
}
/// Called by `config/spec` to get specification configuration.
pub async fn get_spec(network_spec: Arc<NetworkSpec>) -> Result<impl Reply, Rejection> {
    let spec_config = SpecConfig::from_network_spec(&network_spec);

    Ok(with_status(Data::json(spec_config), StatusCode::OK))
}
