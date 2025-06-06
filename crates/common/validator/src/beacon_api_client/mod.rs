pub mod event;
pub mod http_client;
use std::{pin::Pin, time::Duration};

use event::{BeaconEvent, EventTopic};
use eventsource_client::{Client, ClientBuilder, SSE};
use futures::{Stream, StreamExt};
use http_client::{ClientWithBaseUrl, ContentType};
use ream_beacon_api_types::{
    duties::{AttesterDuty, ProposerDuty, SyncCommitteeDuty},
    error::ValidatorError,
    id::{ID, ValidatorID},
    request::ValidatorsPostRequest,
    responses::{
        BeaconResponse, DataResponse, DutiesResponse, ETH_CONSENSUS_VERSION_HEADER,
        SyncCommitteeDutiesResponse, VERSION,
    },
    sync::SyncStatus,
    validator::{ValidatorData, ValidatorStatus},
};
use ream_consensus::{
    attestation_data::AttestationData, fork::Fork, genesis::Genesis,
    single_attestation::SingleAttestation,
};
use ream_network_spec::networks::NetworkSpec;
use reqwest::Url;
use serde_json::json;
use tracing::{error, info};

#[derive(Clone)]
pub struct BeaconApiClient {
    http_client: ClientWithBaseUrl,
}

impl BeaconApiClient {
    pub fn new(beacon_api_endpoint: Url, request_timeout: Duration) -> anyhow::Result<Self> {
        Ok(Self {
            http_client: ClientWithBaseUrl::new(
                beacon_api_endpoint,
                request_timeout,
                ContentType::Ssz,
            )?,
        })
    }

    pub fn get_events_stream(
        &self,
        topics: &[EventTopic],
        stream_tag: &'static str,
    ) -> anyhow::Result<Pin<Box<dyn Stream<Item = BeaconEvent> + Send>>> {
        let endpoint = self.http_client.base_url().join(&format!(
            "/eth/v1/events?topics={}",
            topics
                .iter()
                .map(|topic| topic.to_string())
                .collect::<Vec<_>>()
                .join(",")
        ))?;

        Ok(ClientBuilder::for_url(endpoint.as_str())?
            .build()
            .stream()
            .filter_map(move |event| async move {
                let event = match event {
                    Ok(SSE::Event(event)) => event,
                    Ok(SSE::Connected(connection_details)) => {
                        info!("{stream_tag}: Connected to SSE stream: {connection_details:?}");
                        return None;
                    }
                    Ok(SSE::Comment(comment)) => {
                        info!("{stream_tag}: Received comment: {comment:?}");
                        return None;
                    }
                    Err(err) => {
                        error!("{stream_tag}: Error receiving event: {err:?}");
                        return None;
                    }
                };
                match BeaconEvent::try_from(event) {
                    Ok(event) => Some(event),
                    Err(err) => {
                        error!("{stream_tag}: Failed to decode event: {err:?}");
                        None
                    }
                }
            })
            .boxed())
    }

    pub async fn get_genesis(&self) -> anyhow::Result<DataResponse<Genesis>, ValidatorError> {
        let response = self
            .http_client
            .execute(
                self.http_client
                    .get("/eth/v1/beacon/genesis".to_string())?
                    .build()?,
            )
            .await?;

        if !response.status().is_success() {
            return Err(ValidatorError::RequestFailed {
                status_code: response.status(),
            });
        }

        Ok(response.json().await?)
    }

    pub async fn get_config_spec(
        &self,
    ) -> anyhow::Result<DataResponse<NetworkSpec>, ValidatorError> {
        let response = self
            .http_client
            .execute(
                self.http_client
                    .get("/eth/v1/config/spec".to_string())?
                    .build()?,
            )
            .await?;

        if !response.status().is_success() {
            return Err(ValidatorError::RequestFailed {
                status_code: response.status(),
            });
        }

        Ok(response.json().await?)
    }

    pub async fn get_state_fork(
        &self,
        state_id: ID,
    ) -> anyhow::Result<BeaconResponse<Fork>, ValidatorError> {
        let response = self
            .http_client
            .execute(
                self.http_client
                    .get(format!("/eth/v1/beacon/states/{state_id}/fork"))?
                    .build()?,
            )
            .await?;

        if !response.status().is_success() {
            return Err(ValidatorError::RequestFailed {
                status_code: response.status(),
            });
        }

        Ok(response.json().await?)
    }

    pub async fn get_state_validator_list(
        &self,
        state_id: ID,
        validator_ids: Option<Vec<ValidatorID>>,
        validator_statuses: Option<Vec<ValidatorStatus>>,
    ) -> anyhow::Result<BeaconResponse<Vec<ValidatorData>>, ValidatorError> {
        let response = self
            .http_client
            .execute(
                self.http_client
                    .post(format!("/eth/v1/beacon/states/{state_id}/validators"))?
                    .json(&ValidatorsPostRequest {
                        ids: validator_ids,
                        statuses: validator_statuses,
                    })
                    .build()?,
            )
            .await?;

        if !response.status().is_success() {
            return Err(ValidatorError::RequestFailed {
                status_code: response.status(),
            });
        }

        Ok(response.json().await?)
    }

    pub async fn get_state_validator(
        &self,
        state_id: ID,
        validator_id: ValidatorID,
    ) -> anyhow::Result<BeaconResponse<ValidatorData>, ValidatorError> {
        let response = self
            .http_client
            .execute(
                self.http_client
                    .get(format!(
                        "/eth/v1/beacon/states/{state_id}/validators/{validator_id}"
                    ))?
                    .build()?,
            )
            .await?;

        if !response.status().is_success() {
            return Err(ValidatorError::RequestFailed {
                status_code: response.status(),
            });
        }

        Ok(response.json().await?)
    }

    pub async fn get_node_syncing_status(
        &self,
    ) -> anyhow::Result<DataResponse<SyncStatus>, ValidatorError> {
        let response = self
            .http_client
            .execute(
                self.http_client
                    .get("/eth/v1/node/syncing".to_string())?
                    .build()?,
            )
            .await?;

        if !response.status().is_success() {
            return Err(ValidatorError::RequestFailed {
                status_code: response.status(),
            });
        }

        Ok(response.json().await?)
    }

    pub async fn get_proposer_duties(
        &self,
        epoch: u64,
    ) -> anyhow::Result<DutiesResponse<ProposerDuty>, ValidatorError> {
        let response = self
            .http_client
            .execute(
                self.http_client
                    .get(format!("eth/v1/validator/duties/proposer/{epoch}"))?
                    .build()?,
            )
            .await?;

        if !response.status().is_success() {
            return Err(ValidatorError::RequestFailed {
                status_code: response.status(),
            });
        }

        Ok(response.json().await?)
    }

    pub async fn get_attester_duties(
        &self,
        epoch: u64,
        validator_indices: &[u64],
    ) -> Result<DutiesResponse<AttesterDuty>, ValidatorError> {
        let response = self
            .http_client
            .execute(
                self.http_client
                    .post(format!("/eth/v1/validator/duties/attester/{epoch}"))?
                    .json(&json!(
                        validator_indices
                            .iter()
                            .map(|i| i.to_string())
                            .collect::<Vec<_>>()
                    ))
                    .build()?,
            )
            .await?;

        if !response.status().is_success() {
            return Err(ValidatorError::RequestFailed {
                status_code: response.status(),
            });
        }

        Ok(response.json().await?)
    }

    pub async fn get_sync_committee_duties(
        &self,
        epoch: u64,
        validator_indices: &[u64],
    ) -> Result<SyncCommitteeDutiesResponse<SyncCommitteeDuty>, ValidatorError> {
        let response = self
            .http_client
            .execute(
                self.http_client
                    .post(format!("/eth/v1/validator/duties/sync/{epoch}"))?
                    .json(&json!(
                        validator_indices
                            .iter()
                            .map(|i| i.to_string())
                            .collect::<Vec<_>>()
                    ))
                    .build()?,
            )
            .await?;

        if !response.status().is_success() {
            return Err(ValidatorError::RequestFailed {
                status_code: response.status(),
            });
        }

        Ok(response.json().await?)
    }

    pub async fn get_attestation_data(
        &self,
        slot: u64,
        committee_index: u64,
    ) -> Result<DataResponse<AttestationData>, ValidatorError> {
        let response = self
            .http_client
            .execute(
                self.http_client
                    .get(format!("/eth/v1/validator/attestation_data?slot={slot}&committee_index={committee_index}"))?
                    .build()?,
            )
            .await?;

        if !response.status().is_success() {
            return Err(ValidatorError::RequestFailed {
                status_code: response.status(),
            });
        }

        Ok(response.json().await?)
    }

    pub async fn submit_attestation(
        &self,
        single_attestation: Vec<SingleAttestation>,
    ) -> anyhow::Result<(), ValidatorError> {
        let response = self
            .http_client
            .execute(
                self.http_client
                    .post("/eth/v2/beacon/pool/attestations".to_string())?
                    .header(ETH_CONSENSUS_VERSION_HEADER, VERSION)
                    .json(&single_attestation)
                    .build()?,
            )
            .await?;

        if !response.status().is_success() {
            return Err(ValidatorError::RequestFailed {
                status_code: response.status(),
            });
        }

        Ok(())
    }
}
