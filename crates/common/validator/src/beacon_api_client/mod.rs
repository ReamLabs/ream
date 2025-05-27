pub mod http_client;

use std::{pin::Pin, time::Duration};

use eventsource_client::{Client, ClientBuilder, Event, SSE};
use futures::{Stream, StreamExt};
use http_client::{ClientWithBaseUrl, ContentType};
use ream_executor::ReamExecutor;
use reqwest::Url;
use tracing::{error, info};

pub struct BeaconApiClient {
    http_client: ClientWithBaseUrl,
    async_executor: ReamExecutor,
}

impl BeaconApiClient {
    pub fn new(
        beacon_api_endpoint: Url,
        request_timeout: Duration,
        async_executor: ReamExecutor,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            http_client: ClientWithBaseUrl::new(
                beacon_api_endpoint,
                request_timeout,
                ContentType::Ssz,
            )?,
            async_executor,
        })
    }

    pub fn get_event_stream(
        &self,
        topics: Vec<String>,
    ) -> anyhow::Result<Pin<Box<dyn Stream<Item = Event> + Send>>> {
        let client_builder = ClientBuilder::for_url(
            Url::parse_with_params(
                self.http_client.base_url().join("/eth/v1/events")?.as_str(),
                topics.iter().map(|topic| ("topics", topic.as_str())),
            )?
            .as_str(),
        )?;

        Ok(client_builder
            .build()
            .stream()
            .filter_map(move |event| async move {
                match event {
                    Ok(SSE::Event(event)) => Some(event),
                    Ok(SSE::Connected(connection_details)) => {
                        info!("Connected to SSE stream: {connection_details:?}");
                        None
                    }
                    Ok(SSE::Comment(comment)) => {
                        info!("Received comment: {comment:?}");
                        None
                    }
                    Err(err) => {
                        error!("Error receiving event: {err:?}");
                        None
                    }
                }
            })
            .boxed())
    }

    pub fn start_event_handler(&self) {
        let topics = vec![
            "chain_reorg",
            "attester_slashing",
            "proposer_slashing",
            "voluntary_exit",
        ]
        .into_iter()
        .map(|topic| topic.to_string())
        .collect::<Vec<_>>();
        let stream_result = self.get_event_stream(topics);

        self.async_executor.spawn(async move {
            if let Ok(mut stream) = stream_result {
                while let Some(event) = stream.next().await {
                    match &event.event_type as &str {
                        "chain_reorg" => {
                            info!("Received chain reorg: {event:?}");
                            // TODO
                        }
                        "attester_slashing" => {
                            info!("Received attester slashing: {event:?}");
                            // TODO
                        }
                        "proposer_slashing" => {
                            info!("Received proposer slashing: {event:?}");
                            // TODO
                        }
                        "voluntary_exit" => {
                            info!("Received voluntary exit: {event:?}");
                            // TODO
                        }
                        _ => {
                            info!("Received an unknown event type");
                        }
                    }
                }
            } else {
                error!("Failed to get event stream");
            }
        });
    }

    pub fn start(self) {
        // TODO: add clock for epochs
        // TODO: add the initialization of the key manager server
        self.start_event_handler();
    }
}
