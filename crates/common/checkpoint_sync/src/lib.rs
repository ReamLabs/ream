pub mod checkpoint;

use alloy_primitives::B256;
use anyhow::ensure;
use checkpoint::get_checkpoint_sync_sources;
use ream_consensus::{
    blob_sidecar::{BlobIdentifier, BlobSidecar},
    constants::SECONDS_PER_SLOT,
    electra::{beacon_block::SignedBeaconBlock, beacon_state::BeaconState},
    execution_engine::rpc_types::get_blobs::BlobAndProofV1,
};
use ream_fork_choice::{handlers::on_tick, store::get_forkchoice_store};
use ream_network_spec::networks::network_spec;
use ream_rpc::types::response::BeaconVersionedResponse;
use ream_storage::{db::ReamDB, tables::Table};
use reqwest::{
    Url,
    header::{ACCEPT, HeaderValue},
};
use serde::{Deserialize, Serialize};
use ssz::Decode;
use tracing::{info, warn};

pub const VERSION: &str = "electra";
pub const ETH_CONSENSUS_VERSION_HEADER: &str = "Eth-Consensus-Version";
const EXECUTION_OPTIMISTIC: bool = false;
const FINALIZED: bool = false;

/// A OptionalBeaconVersionedResponse data struct that can be used to wrap data type
/// used for json rpc responses
///
/// # Example
/// {
///  "data": json!({
///     "version": Some("electra")
///     "execution_optimistic" : Some("false"),
///     "finalized" : None,
///     "data" : T
/// })
/// }
#[derive(Debug, Serialize, Deserialize)]
pub struct OptionalBeaconVersionedResponse<T> {
    pub version: Option<String>,
    #[serde(default, deserialize_with = "option_bool_from_str_or_bool")]
    pub execution_optimistic: Option<bool>,
    #[serde(default, deserialize_with = "option_bool_from_str_or_bool")]
    pub finalized: Option<bool>,
    pub data: T,
}

impl<T: Serialize> OptionalBeaconVersionedResponse<T> {
    pub fn new(data: T) -> Self {
        Self {
            version: Some(VERSION.into()),
            data,
            execution_optimistic: Some(EXECUTION_OPTIMISTIC),
            finalized: Some(FINALIZED),
        }
    }
}

fn bool_from_str_or_bool<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct BoolVisitor;

    impl serde::de::Visitor<'_> for BoolVisitor {
        type Value = bool;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a boolean or a string representing a boolean")
        }

        fn visit_bool<E>(self, v: bool) -> Result<Self::Value, E> {
            Ok(v)
        }

        fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            v.parse::<bool>().map_err(E::custom)
        }
    }

    deserializer.deserialize_any(BoolVisitor)
}

fn option_bool_from_str_or_bool<'de, D>(deserializer: D) -> Result<Option<bool>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Some(bool_from_str_or_bool(deserializer)?))
}

/// Entry point for checkpoint sync.
pub async fn initialize_db_from_checkpoint(
    db: ReamDB,
    checkpoint_sync_url: Option<Url>,
) -> anyhow::Result<()> {
    if db.is_initialized() {
        warn!("Starting Checkpoint Sync from existing DB. It is advised to start from a clean DB");
        return Ok(());
    }

    let checkpoint_sync_url = get_checkpoint_sync_sources(checkpoint_sync_url).remove(0);
    info!("Initiating Checkpoint Sync");
    let block = fetch_finalized_block(&checkpoint_sync_url).await?;
    info!(
        "Downloaded Block: {} with Root: {}. Slot: {}",
        block.data.message.body.execution_payload.block_number,
        block.data.message.block_root(),
        block.data.message.slot
    );
    let slot = block.data.message.slot;
    initialize_blobs_in_db(
        &checkpoint_sync_url,
        db.clone(),
        block.data.message.block_root(),
    )
    .await?;
    info!(
        "Downloaded blobs for block: {}",
        block.data.message.body.execution_payload.block_number
    );

    let state = get_state(&checkpoint_sync_url, slot).await?;
    info!(
        "Downloaded State with root: {}. Slot: {}",
        state.data.state_root(),
        slot
    );
    ensure!(block.data.message.slot == state.data.slot, "Slot mismatch");

    ensure!(block.data.message.state_root == state.data.state_root());
    let mut store = get_forkchoice_store(state.data, block.data.message, db)?;

    let time = network_spec().genesis.genesis_time + SECONDS_PER_SLOT * (slot + 1);
    on_tick(&mut store, time)?;
    info!("Initial Sync complete");

    Ok(())
}

/// Fetch initial state from trusted RPC
pub async fn get_state(
    rpc: &Url,
    slot: u64,
) -> anyhow::Result<BeaconVersionedResponse<BeaconState>> {
    let client = reqwest::Client::new();
    let state = client
        .get(format!("{rpc}eth/v2/debug/beacon/states/{slot}"))
        .header(ACCEPT, HeaderValue::from_static("application/octet-stream"))
        .send()
        .await?
        .bytes()
        .await?;

    Ok(BeaconVersionedResponse::new(
        BeaconState::from_ssz_bytes(&state).expect("Unable to decode from SSZ"),
    ))
}

/// Fetch initial block from trusted RPC
pub async fn fetch_finalized_block(
    rpc: &Url,
) -> anyhow::Result<BeaconVersionedResponse<SignedBeaconBlock>> {
    let client = reqwest::Client::new();
    let raw_bytes = client
        .get(format!("{rpc}eth/v2/beacon/blocks/finalized"))
        .header(ACCEPT, HeaderValue::from_static("application/octet-stream"))
        .send()
        .await?
        .bytes()
        .await?;

    Ok(BeaconVersionedResponse::new(
        SignedBeaconBlock::from_ssz_bytes(&raw_bytes).expect("Unable to decode from SSZ"),
    ))
}

// Fetch and initialize blobs in the DB from trusted RPC
pub async fn initialize_blobs_in_db(
    rpc: &Url,
    store: ReamDB,
    beacon_block_root: B256,
) -> anyhow::Result<()> {
    let blob_sidecar = reqwest::get(&format!(
        "{rpc}eth/v1/beacon/blob_sidecars/{beacon_block_root}"
    ))
    .await?
    .json::<OptionalBeaconVersionedResponse<Vec<BlobSidecar>>>()
    .await?;

    for blob_sidecar in blob_sidecar.data {
        store.blobs_and_proofs_provider().insert(
            BlobIdentifier::new(beacon_block_root, blob_sidecar.index),
            BlobAndProofV1 {
                blob: blob_sidecar.blob,
                proof: blob_sidecar.kzg_proof,
            },
        )?;
    }
    Ok(())
}
