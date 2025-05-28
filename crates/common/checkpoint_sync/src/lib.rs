pub mod checkpoint;
pub mod weak_subjectivity;

use alloy_primitives::B256;
use anyhow::{anyhow, ensure};
use checkpoint::get_checkpoint_sync_sources;
use ream_consensus::{
    blob_sidecar::{BlobIdentifier, BlobSidecar},
    checkpoint::Checkpoint,
    constants::SECONDS_PER_SLOT,
    electra::{beacon_block::SignedBeaconBlock, beacon_state::BeaconState},
    execution_engine::rpc_types::get_blobs::BlobAndProofV1,
};
use ream_fork_choice::{handlers::on_tick, store::get_forkchoice_store};
use ream_network_spec::networks::network_spec;
use ream_storage::{db::ReamDB, tables::Table};
use reqwest::{
    Url,
    header::{ACCEPT, HeaderValue},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ssz::Decode;
use tracing::{info, warn};
use weak_subjectivity::{WeakSubjectivityState, verify_state_from_weak_subjectivity_checkpoint};

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
struct OptionalBeaconVersionedResponse<T> {
    pub version: Option<String>,
    pub execution_optimistic: Option<Value>,
    pub finalized: Option<Value>,
    pub data: T,
}

/// Entry point for checkpoint sync.
pub async fn initialize_db_from_checkpoint(
    db: ReamDB,
    checkpoint_sync_url: Option<Url>,
    weak_subjectivity_checkpoint: Option<Checkpoint>,
) -> anyhow::Result<WeakSubjectivityState> {
    if db.is_initialized() {
        warn!("DB is already initialized. Skipping checkpoint sync.");

        let state = db
            .beacon_state_provider()
            .last()?
            .ok_or_else(|| anyhow!("Unable to fetch latest state"))?;

        if let Some(weak_subjectivity_checkpoint) = &weak_subjectivity_checkpoint {
            if !verify_state_from_weak_subjectivity_checkpoint(
                &state,
                weak_subjectivity_checkpoint,
            )? {
                return Ok(WeakSubjectivityState::CheckpointPendingVerification);
            }
        } else {
            return Ok(WeakSubjectivityState::None);
        }
        return Ok(WeakSubjectivityState::CheckpointAlreadyVerified);
    }

    let checkpoint_sync_url = get_checkpoint_sync_sources(checkpoint_sync_url).remove(0);
    info!("Initiating checkpoint sync");

    info!("Fetching finalized block...");
    let block = fetch_finalized_block(&checkpoint_sync_url).await?;
    info!(
        "Downloaded block: {} with root: {}. Slot: {}",
        block.message.body.execution_payload.block_number,
        block.message.block_root(),
        block.message.slot
    );
    let slot = block.message.slot;

    info!("Fetching blobs...");
    initialize_blobs_in_db(&checkpoint_sync_url, db.clone(), block.message.block_root()).await?;
    info!(
        "Downloaded blobs for block: {}",
        block.message.body.execution_payload.block_number
    );

    info!("Fetching initial state...");
    let state = get_state(&checkpoint_sync_url, slot).await?;
    info!(
        "Downloaded state with root: {}. Slot: {}",
        state.state_root(),
        slot
    );

    ensure!(block.message.slot == state.slot, "Slot mismatch");

    ensure!(block.message.state_root == state.state_root());
    let mut store = get_forkchoice_store(state.clone(), block.message, db)?;

    let time = network_spec().min_genesis_time + SECONDS_PER_SLOT * (slot + 1);
    on_tick(&mut store, time)?;
    info!("Initial sync complete");

    if let Some(weak_subjectivity_checkpoint) = &weak_subjectivity_checkpoint {
        if !verify_state_from_weak_subjectivity_checkpoint(&state, weak_subjectivity_checkpoint)? {
            return Ok(WeakSubjectivityState::CheckpointPendingVerification);
        }
    } else {
        return Ok(WeakSubjectivityState::None);
    }
    Ok(WeakSubjectivityState::CheckpointAlreadyVerified)
}

/// Fetch initial state from trusted RPC
async fn get_state(rpc: &Url, slot: u64) -> anyhow::Result<BeaconState> {
    let client = reqwest::Client::new();
    let state = client
        .get(format!("{rpc}eth/v2/debug/beacon/states/{slot}"))
        .header(ACCEPT, HeaderValue::from_static("application/octet-stream"))
        .send()
        .await?
        .bytes()
        .await?;

    BeaconState::from_ssz_bytes(&state)
        .map_err(|err| anyhow!("Unable to decode state from ssz bytes: {err:?}"))
}

/// Fetch initial block from trusted RPC
async fn fetch_finalized_block(rpc: &Url) -> anyhow::Result<SignedBeaconBlock> {
    let client = reqwest::Client::new();
    let raw_bytes = client
        .get(format!("{rpc}eth/v2/beacon/blocks/finalized"))
        .header(ACCEPT, HeaderValue::from_static("application/octet-stream"))
        .send()
        .await?
        .bytes()
        .await?;

    SignedBeaconBlock::from_ssz_bytes(&raw_bytes)
        .map_err(|err| anyhow!("Unable to decode block from ssz bytes: {err:?}"))
}

#[derive(Debug, Serialize, Deserialize)]
struct BlobSidercars {
    pub data: Vec<BlobSidecar>,
}

// Fetch and initialize blobs in the DB from trusted RPC
async fn initialize_blobs_in_db(
    rpc: &Url,
    store: ReamDB,
    beacon_block_root: B256,
) -> anyhow::Result<()> {
    let blob_sidecar = reqwest::get(&format!(
        "{rpc}eth/v1/beacon/blob_sidecars/{beacon_block_root}"
    ))
    .await?
    .json::<BlobSidercars>()
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
