use anyhow::{Result, anyhow};
use ream_consensus_lean::state::LeanState;
use reqwest::{Client, StatusCode, Url};
use ssz::Decode;

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
    if state.slot == 0 {
        return false;
    }

    if state.validators.is_empty() {
        return false;
    }

    true
}
