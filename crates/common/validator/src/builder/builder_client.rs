use std::time::Duration;

use alloy_primitives::B256;
use anyhow::{Ok, anyhow};
use ream_beacon_api_types::responses::{ETH_CONSENSUS_VERSION_HEADER, VERSION};
use ream_bls::PublicKey;
use ream_consensus_beacon::electra::blinded_beacon_block::SignedBlindedBeaconBlock;
use reqwest::StatusCode;
use url::Url;

use super::{
    blobs::ExecutionPayloadAndBlobsBundle, builder_bid::SignedBuilderBid,
    validator_registration::SignedValidatorRegistrationV1,
};
use crate::beacon_api_client::http_client::{ClientWithBaseUrl, ContentType};

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

    /// Get an execution payload header.
    pub async fn get_builder_header(
        &self,
        parent_hash: B256,
        public_key: &PublicKey,
        slot: u64,
    ) -> anyhow::Result<SignedBuilderBid> {
        Ok(self
            .client
            .get(format!(
                "/eth/v1/builder/header/{slot}/{parent_hash:?}/{public_key:?}"
            ))?
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
        let response = self
            .client
            .post(
                "/eth/v1/builder/blinded_blocks".to_string(),
                ContentType::Json,
            )?
            .header(ETH_CONSENSUS_VERSION_HEADER, VERSION)
            .json(&signed_blinded_block)
            .send()
            .await?;

        Ok(response.json::<ExecutionPayloadAndBlobsBundle>().await?)
    }

    /// Check if builder is healthy.
    pub async fn get_builder_status(&self) -> anyhow::Result<()> {
        let response = self.client.get("/eth/v1/builder/status")?.send().await?;
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
        let response = self
            .client
            .post(
                "/eth/v1/builder/register_validator".to_string(),
                ContentType::Json,
            )?
            .header(ETH_CONSENSUS_VERSION_HEADER, VERSION)
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
