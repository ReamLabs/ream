use std::{sync::Arc, time::Duration};

use alloy_primitives::Address;
use anyhow::anyhow;
use ream_bls::{PrivateKey, PubKey};
use ream_consensus::electra::beacon_state::BeaconState;
use ream_keystore::keystore::Keystore;
use ream_network_spec::networks::NetworkSpec;
use reqwest::Url;

use crate::{beacon_api_client::BeaconApiClient, validator_statuses::ValidatorStatus};

pub fn check_if_validator_active(
    state: &BeaconState,
    validator_index: u64,
) -> anyhow::Result<bool> {
    state
        .validators
        .get(validator_index as usize)
        .map(|validator| validator.is_active_validator(state.get_current_epoch()))
        .ok_or_else(|| anyhow!("Validator index out of bounds"))
}

pub fn is_proposer(state: &BeaconState, validator_index: u64) -> anyhow::Result<bool> {
    Ok(state.get_beacon_proposer_index(None)? == validator_index)
}

pub struct ValidatorInfo {
    pub private_key: PrivateKey,
    pub public_key: PubKey,
    pub validator_index: Option<u64>,
    pub validator_status: Option<ValidatorStatus>,
}

impl ValidatorInfo {
    pub fn from_keystore(keystore: Keystore) -> Self {
        Self {
            private_key: keystore.private_key,
            public_key: keystore.public_key,
            validator_index: None,
            validator_status: None,
        }
    }
}

pub struct ValidatorService {
    pub beacon_api_client: BeaconApiClient,
    pub validators: Vec<ValidatorInfo>,
    pub suggested_fee_recipient: Address,
    pub network: Arc<NetworkSpec>,
}

impl ValidatorService {
    pub fn new(
        keystores: Vec<Keystore>,
        suggested_fee_recipient: Address,
        network: Arc<NetworkSpec>,
        beacon_api_endpoint: Url,
        request_timeout: Duration,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            beacon_api_client: BeaconApiClient::new(beacon_api_endpoint, request_timeout)?,
            validators: keystores
                .into_iter()
                .map(ValidatorInfo::from_keystore)
                .collect::<Vec<_>>(),
            suggested_fee_recipient,
            network,
        })
    }
}
