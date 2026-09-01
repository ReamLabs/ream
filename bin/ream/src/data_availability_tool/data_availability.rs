//! A minimal HTTP client for the data node's `/data/v0` RPC — the same wire
//! surface the beacon-side feeder will speak.

use alloy_primitives::B256;
use anyhow::{Context, Result, anyhow, bail};
use reqwest::StatusCode;
use serde_json::{Value, json};
use ssz::Encode;
use ssz_derive::{Decode as SszDecode, Encode as SszEncode};
use ssz_types::{VariableList, typenum};

/// How long to keep retrying a `503 Service Unavailable` from the ingest
/// endpoints (the data node sheds load when its verification queue is full; that
/// is a retry-shortly signal, not a failure).
const OVERLOAD_RETRIES: u32 = 50;
const OVERLOAD_BACKOFF_MS: u64 = 200;

/// One `(column index, payload)` entry of the SSZ batch-ingest body.
#[derive(SszEncode, SszDecode)]
struct WireIndexedPayload {
    index: u64,
    payload: VariableList<u8, typenum::U16777216>,
}

/// SSZ body of `POST /data/v0/ingest/block/{block_root}`.
type WireBlockBatch = VariableList<WireIndexedPayload, typenum::U128>;

pub struct DataAvailabilityClient {
    http: reqwest::Client,
    base: String,
}

impl DataAvailabilityClient {
    pub fn new(base_url: String) -> Self {
        Self {
            http: reqwest::Client::new(),
            base: base_url.trim_end_matches('/').to_string(),
        }
    }

    /// `GET /data/v0/health`
    pub async fn health(&self) -> Result<Value> {
        self.get_json(&format!("{}/data/v0/health", self.base))
            .await
    }

    /// `GET /data/v0/availability/{block_root}`
    pub async fn availability(&self, block_root: B256) -> Result<Value> {
        self.get_json(&format!("{}/data/v0/availability/{block_root}", self.base))
            .await
    }

    /// `POST /data/v0/ingest` — submit one column payload (hex-encoded SSZ) for
    /// verification. Backs off and retries while the data node reports overload.
    pub async fn ingest(
        &self,
        block_root: B256,
        index: u64,
        slot: u64,
        payload: &str,
    ) -> Result<()> {
        let url = format!("{}/data/v0/ingest", self.base);
        let body = json!({
            "block_root": block_root,
            "index": index,
            "slot": slot,
            "payload": payload,
        });

        for _ in 0..OVERLOAD_RETRIES {
            let response = self
                .http
                .post(&url)
                .json(&body)
                .send()
                .await
                .with_context(|| format!("POST {url}"))?;

            match response.status() {
                StatusCode::ACCEPTED => return Ok(()),
                StatusCode::SERVICE_UNAVAILABLE => {
                    tokio::time::sleep(std::time::Duration::from_millis(OVERLOAD_BACKOFF_MS)).await;
                }
                status => {
                    let body = response.text().await.unwrap_or_default();
                    bail!("data node rejected column {index}: {status}: {body}");
                }
            }
        }
        bail!("data node still overloaded after {OVERLOAD_RETRIES} retries");
    }

    /// `POST /data/v0/ingest/block/{block_root}?slot={slot}` — submit a whole
    /// block's columns as one SSZ batch: raw payload bytes.
    pub async fn ingest_block(
        &self,
        block_root: B256,
        slot: u64,
        columns: &[(u64, Vec<u8>)],
    ) -> Result<()> {
        let entries: Vec<WireIndexedPayload> = columns
            .iter()
            .map(|(index, payload)| {
                Ok(WireIndexedPayload {
                    index: *index,
                    payload: VariableList::new(payload.clone())
                        .map_err(|err| anyhow!("column payload too large: {err:?}"))?,
                })
            })
            .collect::<Result<_>>()?;
        let batch: WireBlockBatch = VariableList::new(entries)
            .map_err(|err| anyhow!("too many columns for one block: {err:?}"))?;
        let body = batch.as_ssz_bytes();

        let url = format!(
            "{}/data/v0/ingest/block/{block_root}?slot={slot}",
            self.base
        );
        for _ in 0..OVERLOAD_RETRIES {
            let response = self
                .http
                .post(&url)
                .header("content-type", "application/octet-stream")
                .body(body.clone())
                .send()
                .await
                .with_context(|| format!("POST {url}"))?;

            match response.status() {
                StatusCode::ACCEPTED => return Ok(()),
                StatusCode::SERVICE_UNAVAILABLE => {
                    tokio::time::sleep(std::time::Duration::from_millis(OVERLOAD_BACKOFF_MS)).await;
                }
                status => {
                    let body = response.text().await.unwrap_or_default();
                    bail!("data node rejected the batch: {status}: {body}");
                }
            }
        }
        bail!("data node still overloaded after {OVERLOAD_RETRIES} retries");
    }

    async fn get_json(&self, url: &str) -> Result<Value> {
        let response = self
            .http
            .get(url)
            .send()
            .await
            .with_context(|| format!("GET {url} — is the data node running?"))?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            bail!("data node returned {status} for {url}: {body}");
        }
        Ok(response.json().await?)
    }
}
