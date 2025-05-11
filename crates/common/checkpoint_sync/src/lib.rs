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
use ream_rpc::types::response::{BeaconVersionedResponse, OptionalBeaconVersionedResponse};
use ream_storage::{db::ReamDB, tables::Table};
use reqwest::{
    Url,
    header::{ACCEPT, HeaderValue},
};
use ssz::Decode;
use tracing::{info, warn};

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
) -> anyhow::Result<OptionalBeaconVersionedResponse<SignedBeaconBlock>> {
    Ok(reqwest::get(format!("{rpc}eth/v2/beacon/blocks/finalized"))
        .await?
        .json::<OptionalBeaconVersionedResponse<SignedBeaconBlock>>()
        .await?)
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
