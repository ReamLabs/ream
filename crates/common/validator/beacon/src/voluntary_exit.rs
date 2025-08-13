use std::time::Duration;

use anyhow::{anyhow, bail};
use ream_api_types_beacon::{
    error::ValidatorError,
    id::{ID, ValidatorID},
    validator::ValidatorStatus,
};
use ream_bls::{PrivateKey, traits::Signable};
use ream_consensus_beacon::voluntary_exit::{SignedVoluntaryExit, VoluntaryExit};
use ream_consensus_misc::{
    constants::beacon::DOMAIN_VOLUNTARY_EXIT,
    misc::{compute_domain, compute_signing_root},
};
use ream_network_spec::networks::beacon_network_spec;
use tokio::time::sleep;
use tracing::info;

use crate::beacon_api_client::BeaconApiClient;

pub fn sign_voluntary_exit(
    epoch: u64,
    validator_index: u64,
    private_key: &PrivateKey,
) -> anyhow::Result<SignedVoluntaryExit> {
    let voluntary_exit = VoluntaryExit {
        epoch,
        validator_index,
    };

    Ok(SignedVoluntaryExit {
        signature: private_key
            .sign(
                compute_signing_root(
                    &voluntary_exit,
                    compute_domain(
                        DOMAIN_VOLUNTARY_EXIT,
                        Some(beacon_network_spec().electra_fork_version),
                        None,
                    ),
                )
                .as_ref(),
            )
            .map_err(|err| anyhow!("Failed to sign voluntary exit: {err}"))?,
        message: voluntary_exit,
    })
}

pub async fn process_voluntary_exit(
    beacon_api_client: &BeaconApiClient,
    validator_index: u64,
    epoch: u64,
    private_key: &PrivateKey,
    wait_till_exit: bool,
) -> anyhow::Result<()> {
    if beacon_api_client
        .get_node_syncing_status()
        .await?
        .data
        .is_syncing
    {
        bail!("Cannot process voluntary exit while node is syncing");
    }

    if let Err(err) = beacon_api_client
        .submit_signed_voluntary_exit(sign_voluntary_exit(epoch, validator_index, private_key)?)
        .await
    {
        match err {
            ValidatorError::RequestFailedWithMessage { message, .. } => {
                bail!("Failed to submit voluntary exit: {message}");
            }
            _ => bail!("Failed to submit voluntary exit: {err}"),
        }
    }

    if wait_till_exit {
        loop {
            sleep(Duration::from_secs(beacon_network_spec().seconds_per_slot)).await;
            match beacon_api_client
                .get_state_validator(ID::Head, ValidatorID::Index(validator_index))
                .await?
                .data
                .status
            {
                ValidatorStatus::ActiveExiting => {
                    info!(
                        "Voluntary exit has been published to beacon chain but validator has not yet exited."
                    );
                }
                ValidatorStatus::ExitedSlashed | ValidatorStatus::ExitedUnslashed => {
                    info!("Validator has successfully exited");
                    break;
                }
                _ => {
                    info!("Voluntary exit has not yet been published to beacon chain.");
                }
            }
        }
    }

    Ok(())
}
