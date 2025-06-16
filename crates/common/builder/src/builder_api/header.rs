use alloy_primitives::B256;
use anyhow::Ok;
use ream_bls::PubKey;

use crate::{BuilderConfig, builder_bid::SignedBuilderBid};

pub async fn get_builder_header(
    config: BuilderConfig,
    parent_hash: B256,
    pubkey: &PubKey,
    slot: u64,
) -> anyhow::Result<SignedBuilderBid> {
    let get_header_endpoint = config.mev_relay_url.join(&format!(
        "/eth/v1/builder/header/{slot}/{parent_hash:?}/{pubkey:?}"
    ))?;
    let signed_blinded_bid = reqwest::Client::new()
        .get(get_header_endpoint)
        .send()
        .await?
        .json::<SignedBuilderBid>()
        .await?;

    Ok(signed_blinded_bid)
}
