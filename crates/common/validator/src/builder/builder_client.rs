use std::time::Duration;

use alloy_primitives::B256;
use anyhow::{Ok, anyhow};
use ream_beacon_api_types::responses::{ETH_CONSENSUS_VERSION_HEADER, VERSION};
use ream_bls::PublicKey;
use ream_consensus::electra::blinded_beacon_block::SignedBlindedBeaconBlock;
use reqwest::{
    StatusCode,
    header::{CONTENT_TYPE, HeaderMap, HeaderValue},
};
use url::Url;

use super::{
    blobs::ExecutionPayloadAndBlobsBundle, builder_bid::SignedBuilderBid,
    validator_registration::SignedValidatorRegistrationV1,
};
use crate::beacon_api_client::http_client::{ClientWithBaseUrl, ContentType, JSON_CONTENT_TYPE};

#[derive(Debug, Clone)]
pub struct BuilderConfig {
    pub builder_enabled: bool,
    pub mev_relay_url: Url,
}

pub struct BuilderClient {
    client: ClientWithBaseUrl,
}

impl BuilderClient {
    pub fn new(
        config: BuilderConfig,
        request_timeout: Duration,
        content_type: ContentType,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            client: ClientWithBaseUrl::new(config.mev_relay_url, request_timeout, content_type)?,
        })
    }

    pub fn get_header_with_json(&self) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static(JSON_CONTENT_TYPE));
        headers.insert(
            ETH_CONSENSUS_VERSION_HEADER,
            HeaderValue::from_static(VERSION),
        );
        headers
    }

    /// Get an execution payload header.
    pub async fn get_builder_header(
        &self,
        parent_hash: B256,
        public_key: &PublicKey,
        slot: u64,
    ) -> anyhow::Result<SignedBuilderBid> {
        let get_header_endpoint = self.client.base_url().join(&format!(
            "/eth/v1/builder/header/{slot}/{parent_hash:?}/{public_key:?}"
        ))?;

        Ok(self
            .client
            .get(get_header_endpoint)?
            .send()
            .await?
            .json::<SignedBuilderBid>()
            .await?)
    }

    /// Submit a signed blinded block and get unblinded execution payload.
    pub async fn get_blinded_blocks(
        &self,
        signed_blinded_block: SignedBlindedBeaconBlock,
    ) -> anyhow::Result<ExecutionPayloadAndBlobsBundle> {
        let get_blinded_blocks_endpoint = self
            .client
            .base_url()
            .join("/eth/v1/builder/blinded_blocks")?;

        let response = self
            .client
            .post(get_blinded_blocks_endpoint, ContentType::Json)?
            .headers(self.get_header_with_json())
            .json(&signed_blinded_block)
            .send()
            .await?;

        Ok(response.json::<ExecutionPayloadAndBlobsBundle>().await?)
    }

    /// Check if builder is healthy.
    pub async fn get_builder_status(&self) -> anyhow::Result<()> {
        let builder_statis_endpoint = self.client.base_url().join("/eth/v1/builder/status")?;

        let response = self.client.get(builder_statis_endpoint)?.send().await?;
        match response.status() {
            StatusCode::OK => Ok(()),
            StatusCode::INTERNAL_SERVER_ERROR => {
                Err(anyhow!("internal error: builder internal error"))
            }
            status => Err(anyhow!("failed to get builder status: {status:?}")),
        }
    }

    /// Registers a validator's preferred fee recipient and gas limit.
    pub async fn resgister_validator(
        &self,
        signed_registration: SignedValidatorRegistrationV1,
    ) -> anyhow::Result<()> {
        let register_validator_endpoint = self
            .client
            .base_url()
            .join("/eth/v1/builder/register_validator")?;

        let response = self
            .client
            .post(register_validator_endpoint, ContentType::Json)?
            .headers(self.get_header_with_json())
            .json(&signed_registration)
            .send()
            .await?;

        match response.status() {
            StatusCode::OK => Ok(()),
            StatusCode::BAD_REQUEST => Err(anyhow!("unknown validator")),
            StatusCode::UNSUPPORTED_MEDIA_TYPE => Err(anyhow!("unsupported media type")),
            StatusCode::INTERNAL_SERVER_ERROR => Err(anyhow!("builder internal error")),
            status => Err(anyhow!("internal error: {status:?}")),
        }
    }
}
