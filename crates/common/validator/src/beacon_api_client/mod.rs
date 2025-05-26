pub mod http_client;

use std::time::Duration;

use anyhow;
use http_client::{ClientWithBaseUrl, ContentType};
use reqwest::Url;

pub struct BeaconApiClient {
    pub http_client: ClientWithBaseUrl,
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
}
