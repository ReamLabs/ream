use alloy_primitives::B256;
use anyhow::Ok;
use ream_bls::PubKey;
use url::Url;

use crate::{BuilderConfig, builder_bid::SignedBuilderBid};

pub struct BuilderClient {
    client: reqwest::Client,
    mev_relay_url: Url,
}

impl BuilderClient {
    pub fn new(config: BuilderConfig) -> anyhow::Result<Self> {
        Ok(Self {
            client: reqwest::Client::new(),
            mev_relay_url: config.mev_relay_url,
        })
    }

    pub async fn get_builder_header(
        &self,
        parent_hash: B256,
        public_key: &PubKey,
        slot: u64,
    ) -> anyhow::Result<SignedBuilderBid> {
        let get_header_endpoint = self.mev_relay_url.join(&format!(
            "/eth/v1/builder/header/{slot}/{parent_hash:?}/{public_key:?}"
        ))?;

        Ok(self
            .client
            .get(get_header_endpoint)
            .send()
            .await?
            .json::<SignedBuilderBid>()
            .await?)
    }
}
