use anyhow::{Result, anyhow};
use ream_consensus_lean::state::LeanState;
use ream_consensus_misc::constants::lean::VALIDATOR_REGISTRY_LIMIT;
use reqwest::{Client, StatusCode, Url};
use ssz::Decode;
use tracing::warn;

#[derive(Default)]
pub struct LeanCheckpointClient {
    http: Client,
}

impl LeanCheckpointClient {
    pub fn new() -> Self {
        Self {
            http: Client::builder()
                .build()
                .expect("Failed to build HTTP client"),
        }
    }

    pub async fn fetch_finalized_state(&self, url: &Url) -> Result<LeanState> {
        let url = url.join("/lean/v0/states/finalized")?;

        let response = self
            .http
            .get(url)
            .header("Accept", "application/octet-stream")
            .send()
            .await?;

        if response.status() != StatusCode::OK {
            return Err(anyhow!(
                "HTTP error {}: {}",
                response.status(),
                response.text().await?
            ));
        }

        LeanState::from_ssz_bytes(&response.bytes().await?)
            .map_err(|err| anyhow!("SSZ decode failed: {err:?}"))
    }
}

pub fn verify_checkpoint_state(state: &LeanState) -> bool {
    if state.validators.is_empty() {
        warn!("Invalid state: no validators in registry");
        return false;
    }

    let validator_count = state.validators.len() as u64;
    if state.validators.len() > VALIDATOR_REGISTRY_LIMIT as usize {
        warn!(
            "Invalid state: validator count {} exceeds registry limit {}",
            validator_count, VALIDATOR_REGISTRY_LIMIT,
        );
        return false;
    }

    true
}
