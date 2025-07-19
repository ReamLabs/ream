use std::time::Duration;

use anyhow::anyhow;
use ream_beacon_api_types::{
    id::{ID, ValidatorID},
    validator::ValidatorStatus,
};
use ream_bls::{PrivateKey, traits::Signable};
use ream_consensus::{
    constants::DOMAIN_VOLUNTARY_EXIT,
    misc::{compute_domain, compute_signing_root},
    voluntary_exit::{SignedVoluntaryExit, VoluntaryExit},
};
use ream_network_spec::networks::network_spec;
use tokio::time::sleep;

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
                        Some(network_spec().electra_fork_version),
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
    let sync_status = beacon_api_client.get_node_syncing_status().await?;

    if sync_status.data.is_syncing {
        return Err(anyhow!(
            "Cannot process voluntary exit while node is syncing"
        ));
    }

    let signed_voluntary_exit = sign_voluntary_exit(epoch, validator_index, private_key)?;
    beacon_api_client
        .submit_signed_voluntary_exit(signed_voluntary_exit)
        .await?;

    if wait_till_exit {
        loop {
            match beacon_api_client
                .get_state_validator(ID::Head, ValidatorID::Index(validator_index))
                .await?
                .data
                .status
            {
                ValidatorStatus::ActiveExiting => {
                    println!(
                        "Voluntary exit has been published to beacon chain but validator has not yet exited."
                    );
                    sleep(Duration::from_secs(network_spec().seconds_per_slot)).await;
                }
                ValidatorStatus::ExitedSlashed | ValidatorStatus::ExitedUnslashed => {
                    println!("Validator has successfully exited");
                    break;
                }
                _ => {
                    println!("Voluntary exit has not yet been published to beacon chain.");
                    sleep(Duration::from_secs(network_spec().seconds_per_slot)).await;
                }
            }
        }
    }

    Ok(())
}
