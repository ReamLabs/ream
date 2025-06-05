use std::{
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use alloy_primitives::Address;
use anyhow::anyhow;
use ream_bls::{PrivateKey, PubKey};
use ream_consensus::{electra::beacon_state::BeaconState, misc::compute_epoch_at_slot};
use ream_executor::ReamExecutor;
use ream_keystore::keystore::Keystore;
use ream_network_spec::networks::NetworkSpec;
use reqwest::Url;
use tokio::{
    task::JoinHandle,
    time::{Instant, MissedTickBehavior, interval_at},
};
use tracing::info;

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
    pub beacon_api_client: Arc<BeaconApiClient>,
    pub validators: Vec<Arc<ValidatorInfo>>,
    pub suggested_fee_recipient: Arc<Address>,
    pub network: Arc<NetworkSpec>,
    pub executor: ReamExecutor,
}

impl ValidatorService {
    pub fn new(
        keystores: Vec<Keystore>,
        suggested_fee_recipient: Address,
        network: Arc<NetworkSpec>,
        beacon_api_endpoint: Url,
        request_timeout: Duration,
        executor: ReamExecutor,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            beacon_api_client: Arc::new(BeaconApiClient::new(
                beacon_api_endpoint,
                request_timeout,
            )?),
            validators: keystores
                .into_iter()
                .map(|keystore| Arc::new(ValidatorInfo::from_keystore(keystore)))
                .collect::<Vec<_>>(),
            suggested_fee_recipient: Arc::new(suggested_fee_recipient),
            network,
            executor,
        })
    }

    pub fn start_clock(&self, genesis_time: u64) -> JoinHandle<()> {
        let seconds_per_slot = self.network.seconds_per_slot;
        let genesis_instant = UNIX_EPOCH + Duration::from_secs(genesis_time);
        let elapsed = SystemTime::now()
            .duration_since(genesis_instant)
            .expect("System Time is before the genesis time");

        let mut slot = elapsed.as_secs() / seconds_per_slot;
        let mut epoch = compute_epoch_at_slot(slot);

        let mut interval = {
            let interval_start =
                Instant::now() - (elapsed - Duration::from_secs(slot * seconds_per_slot));
            interval_at(interval_start, Duration::from_secs(seconds_per_slot))
        };
        interval.set_missed_tick_behavior(MissedTickBehavior::Burst);

        self.executor.spawn(async move {
            Self::on_epoch(epoch);
            Self::on_slot(slot);
            loop {
                interval.tick().await;

                slot += 1;
                let current_epoch = compute_epoch_at_slot(slot);

                if current_epoch != epoch {
                    epoch = current_epoch;
                    Self::on_epoch(epoch);
                }
                Self::on_slot(slot);
            }
        })
    }

    pub async fn start(&self) {
        let genesis_info = self
            .beacon_api_client
            .get_genesis()
            .await
            .expect("Could not retrieve genesis information");
        let clock_handle = self.start_clock(genesis_info.data.genesis_time);

        tokio::select! {
            _ = clock_handle => {
                info!("Clock Service stopped!");
            },
        }
    }

    pub fn on_slot(slot: u64) {
        println!("Current Slot: {}", slot);
    }

    pub fn on_epoch(epoch: u64) {
        println!("Current Epoch: {}", epoch);
    }
}
