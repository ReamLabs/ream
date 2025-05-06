use alloy_primitives::B256;
use anyhow::{anyhow, ensure};
use checkpoint::fetch_default_checkpoint_url;
use ream_consensus::{
    blob_sidecar::{BlobIdentifier, BlobSidecar},
    constants::SECONDS_PER_SLOT,
    electra::{beacon_block::SignedBeaconBlock, beacon_state::BeaconState},
    execution_engine::rpc_types::get_blobs::BlobAndProofV1,
    misc::{compute_epoch_at_slot, compute_start_slot_at_epoch},
};
use ream_fork_choice::{
    handlers::on_tick,
    store::{Store, get_forkchoice_store},
};
use ream_network_spec::networks::network_spec;
use ream_rpc::types::response::BeaconVersionedResponse;
use ream_storage::{db::ReamDB, tables::Table};
use tracing::{info, warn};
pub mod checkpoint;

/// Entry point for checkpoint sync.
pub async fn initialize_db_from_checkpoint(
    db: ReamDB,
    rpc: Option<String>,
) -> anyhow::Result<(Store, u64)> {
    let checkpoint_sync_url = match rpc {
        Some(url) => url,
        // 0 for Mainnet, 1 for Sepolia
        None => fetch_default_checkpoint_url().expect("Unable to fetch default checkpoint url")[1]
            .checkpoint_urls[0]
            .clone(),
    };

    info!("Starting Checkpoint Sync");
    let current_slot = db
        .slot_index_provider()
        .get_highest_slot()
        .map_err(|err| anyhow!("Unable to fetch highest slot in db:{}", err))?;

    if current_slot.is_some() {
        warn!("Starting Checkpoint Sync from existing DB. It is advised to start from a fresh DB");
    }
    let block = fetch_finalized_block(&checkpoint_sync_url, current_slot).await?;
    let slot = block.data.message.slot;
    fetch_blobs(
        &checkpoint_sync_url,
        db.clone(),
        block.data.message.block_hash(),
    )
    .await?;

    let state = fetch_finalized_state(&checkpoint_sync_url, block.data.message.slot).await?;
    info!("Received block from slot:{}", slot);
    ensure!(
        block.data.message.slot == state.data.slot,
        anyhow::anyhow!("Slot mismatch")
    );

    ensure!(block.data.message.state_root == state.data.state_hash());
    let mut store = get_forkchoice_store(state.data, block.data.message, db)?;

    // using sepolia genesis time until pectra mainnet
    let time = network_spec().genesis.genesis_time + SECONDS_PER_SLOT * (slot + 1);
    on_tick(&mut store, time)?;
    info!("Initial Sync complete");

    Ok((store, slot))
}

/// Fetch initial state from trusted RPC
pub async fn fetch_finalized_state(
    rpc: &str,
    slot: u64,
) -> anyhow::Result<BeaconVersionedResponse<BeaconState>> {
    let body = reqwest::get(format!("{}{}{}", rpc, "/eth/v2/debug/beacon/states/", slot))
        .await?
        .text()
        .await?;

    let state: BeaconVersionedResponse<BeaconState> = serde_json::from_str(&body)?;
    info!("State hash:{}", state.data.state_hash());

    Ok(state)
}

/// Fetch initial block from trusted RPC
pub async fn fetch_finalized_block(
    rpc: &str,
    slot: Option<u64>,
) -> anyhow::Result<BeaconVersionedResponse<SignedBeaconBlock>> {
    let url = match slot {
        Some(slot) => {
            info!("Starting from slot:{}", slot);
            format!("{}{}{}", rpc, "/eth/v2/beacon/blocks/", slot + 1)
        }
        None => {
            format!("{}{}", rpc, "/eth/v2/beacon/blocks/finalized")
        }
    };

    let body = reqwest::get(url).await?.text().await?;

    let mut block: BeaconVersionedResponse<SignedBeaconBlock> = serde_json::from_str(&body)?;

    // verify slot is a start slot of epoch
    // if not,calculate epoch start slot and fetch block at that slot
    let epoch_start_slot =
        compute_start_slot_at_epoch(compute_epoch_at_slot(block.data.message.slot));
    if !epoch_start_slot == block.data.message.slot {
        warn!("Slot {} is not start of epoch", block.data.message.slot);
        let body = reqwest::get(format!(
            "{}{}{}",
            rpc, "/eth/v2/beacon/blocks/", epoch_start_slot
        ))
        .await?
        .text()
        .await?;
        block = serde_json::from_str(&body)?;
    }

    info!("Block hash:{}", block.data.message.block_hash());

    Ok(block)
}

// fetch blobs from trusted RPC
pub async fn fetch_blobs(rpc: &str, store: ReamDB, beacon_block_root: B256) -> anyhow::Result<()> {
    let body = reqwest::get(format!(
        "{}{}{}",
        rpc, "/eth/v1/beacon/blob_sidecars/", beacon_block_root
    ))
    .await?
    .text()
    .await?;

    let blob_sidecar: BeaconVersionedResponse<Vec<BlobSidecar>> = serde_json::from_str(&body)?;
    info!("Got blobs len:{}", blob_sidecar.data.len());
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
