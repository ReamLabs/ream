pub mod event;
pub mod http_client;
pub mod produce_block;

use std::{pin::Pin, time::Duration};

use alloy_primitives::{B256, hex};
use event::{BeaconEvent, EventTopic};
use eventsource_client::{Client, ClientBuilder, SSE};
use futures::{Stream, StreamExt};
use http_client::{ClientWithBaseUrl, ContentType};
use produce_block::ProduceBlock;
use ream_bls::BLSSignature;
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

    pub async fn produce_block(
        &self,
        slot: u64,
        randao_reveal: BLSSignature,
        graffiti: Option<B256>,
        skip_randao_verification: Option<bool>,
        builder_boost_factor: Option<u64>,
    ) -> anyhow::Result<ProduceBlock> {
        let mut request_builder = self
            .http_client
            .get(format!("/eth/v3/validator/blocks/{slot}"))?
            .query(&[("randao_reveal", format!("{:?}", randao_reveal))]);

        if let Some(graffiti_value) = graffiti {
            request_builder = request_builder.query(&[("graffiti", format!("0x{}", hex::encode(graffiti_value)))]);
        }

        if let Some(skip_randao) = skip_randao_verification {
            request_builder =
                request_builder.query(&[("skip_randao_verification", skip_randao.to_string())]);
        }

        if let Some(boost_factor) = builder_boost_factor {
            request_builder =
                request_builder.query(&[("builder_boost_factor", boost_factor.to_string())]);
        }

        let response = self.http_client.execute(request_builder.build()?).await?;

        if response.status().is_success() {
            Ok(response.json().await?)
        } else {
            anyhow::bail!("Failed to produce block: {}", response.status())
        }
    }
}
