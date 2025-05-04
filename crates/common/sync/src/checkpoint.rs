use alloy_primitives::B256;
use anyhow::ensure;
use ream_consensus::{
    blob_sidecar::{BlobIdentifier, BlobSidecar},
    constants::SECONDS_PER_SLOT,
    electra::{beacon_block::SignedBeaconBlock, beacon_state::BeaconState},
    execution_engine::{mock_engine::MockExecutionEngine, rpc_types::get_blobs::BlobAndProofV1},
    misc::{compute_epoch_at_slot, compute_start_slot_at_epoch},
};
use ream_fork_choice::{
    handlers::{on_block, on_tick},
    store::{Store, get_forkchoice_store},
};
use ream_rpc::types::response::BeaconVersionedResponse;
use ream_storage::{db::ReamDB, tables::Table};
use tracing::{info, warn};

/// Entry point for checkpoint sync.
pub async fn checkpoint_sync(db: ReamDB, rpc: &str) -> anyhow::Result<(Store, u64)> {
    info!("Starting Checkpoint Sync");

    let block = fetch_finalized_block(rpc).await?;
    let mut slot = block.data.message.slot;
    fetch_blobs(rpc, db.clone(), block.data.message.block_hash()).await?;

    let state = fetch_finalized_state(rpc, block.data.message.slot).await?;
    info!("Slot:{}", slot);
    ensure!(
        block.data.message.slot == state.data.slot,
        anyhow::anyhow!("Slot mismatch")
    );

    ensure!(block.data.message.state_root == state.data.state_hash());
    let mut store = get_forkchoice_store(state.data, block.data.message, db)?;

    // using sepolia genesis time until pectra mainnet
    let time = 1655733600 + SECONDS_PER_SLOT * (slot + 1);
    on_tick(&mut store, time)?;
    info!("Initial Sync complete");

    // mocks receiving 5 blocks from peers
    for _ in 0..5 {
        slot = mock_p2p_blocks(rpc, slot + 1, &mut store).await?;
    }

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
) -> anyhow::Result<BeaconVersionedResponse<SignedBeaconBlock>> {
    let body = reqwest::get(format!("{}{}", rpc, "/eth/v2/beacon/blocks/finalized"))
        .await?
        .text()
        .await?;

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

// mock receiving blocks+blobs from peers
pub async fn mock_p2p_blocks(rpc: &str, slot: u64, store: &mut Store) -> anyhow::Result<u64> {
    let body = reqwest::get(format!("{}{}{}", rpc, "/eth/v2/beacon/blocks/", slot))
        .await?
        .text()
        .await?;
    let block: BeaconVersionedResponse<SignedBeaconBlock> = serde_json::from_str(&body)?;
    info!("Got block hash:{}", block.data.message.block_hash());
    info!("Slot: {}", block.data.message.slot);

    // mock receiving blobs from p2p
    fetch_blobs(rpc, store.db.clone(), block.data.message.block_hash()).await?;

    on_block(store, &block.data, &MockExecutionEngine::new()).await?;

    let time = 1655733600 + SECONDS_PER_SLOT * (block.data.message.slot + 1);
    on_tick(store, time)?;

    Ok(block.data.message.slot)
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
