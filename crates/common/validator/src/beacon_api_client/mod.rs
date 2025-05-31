pub mod event;
pub mod http_client;
use std::{pin::Pin, time::Duration};

use event::{BeaconEvent, EventTopic};
use eventsource_client::{Client, ClientBuilder, SSE};
use futures::{Stream, StreamExt};
use http_client::{ClientWithBaseUrl, ContentType};
use ream_beacon_api_types::{
    duties::ProposerDuty,
    error::ValidatorError,
    responses::{DataResponse, DutiesResponse},
    sync::SyncStatus,
};
use reqwest::Url;
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
}
