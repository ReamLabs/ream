pub mod http_client;

use std::time::Duration;

use anyhow;
use http_client::{ClientWithBaseUrl, ContentType};
use ream_beacon_api_types::{
    duties::ProposerDuty, error::ValidatorError, responses::DutiesResponse,
};
use reqwest::Url;
use ssz::Decode;

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

    pub async fn get_proposer_duties(
        &self,
        epoch: u64,
    ) -> Result<DutiesResponse<ProposerDuty>, ValidatorError> {
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

        let proposer_duties = if response
            .headers()
            .get("content-type")
            .and_then(|content_type| content_type.to_str().ok())
            .is_some_and(|content_type| content_type.contains("application/octet-stream"))
        {
            DutiesResponse::from_ssz_bytes(&response.bytes().await?)
                .map_err(|err| ValidatorError::SszDecodeError(format!("{:?}", err)))?
        } else {
            response
                .json()
                .await
                .map_err(|err| ValidatorError::JsonDecodeError(err.to_string()))?
        };

        Ok(proposer_duties)
    }
}
