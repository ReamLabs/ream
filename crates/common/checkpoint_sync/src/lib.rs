pub mod checkpoint;
use alloy_primitives::B256;
use anyhow::{anyhow, ensure};
use checkpoint::fetch_default_checkpoint_url;
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
use tracing::{info, warn};

/// Entry point for checkpoint sync.
pub async fn initialize_db_from_checkpoint(
    db: ReamDB,
    checkpoint_sync_url: Option<String>,
) -> anyhow::Result<()> {
    let checkpoint_sync_url = match checkpoint_sync_url {
        Some(url) => url,
        // 0 for Mainnet, 1 for Sepolia
        None => {
            let checkpoint_urls =
                fetch_default_checkpoint_url().expect("Unable to fetch default checkpo;int url");
            match network_spec().network {
                ream_network_spec::networks::Network::Mainnet => {
                    checkpoint_urls[0].checkpoint_urls[0].clone()
                }
                ream_network_spec::networks::Network::Holesky => {
                    checkpoint_urls[0].checkpoint_urls[1].clone()
                }
                ream_network_spec::networks::Network::Sepolia => {
                    checkpoint_urls[0].checkpoint_urls[2].clone()
                }
                ream_network_spec::networks::Network::Hoodi => {
                    checkpoint_urls[0].checkpoint_urls[3].clone()
                }
                ream_network_spec::networks::Network::Dev => {
                    checkpoint_urls[0].checkpoint_urls[4].clone()
                }
            }
        }
    };

    let current_slot = db
        .slot_index_provider()
        .get_highest_slot()
        .map_err(|err| anyhow!("Unable to fetch highest slot in db:{}", err))?;

    if current_slot.is_some() {
        warn!("Starting Checkpoint Sync from existing DB. It is advised to start from a fresh DB");
        return Ok(());
    }
    info!("Starting Checkpoint Sync");
    let block = fetch_finalized_block(&checkpoint_sync_url).await?;
    let slot = block.data.message.slot;
    fetch_blobs(
        &checkpoint_sync_url,
        db.clone(),
        block.data.message.block_root(),
    )
    .await?;

    let state = get_state(&checkpoint_sync_url, block.data.message.slot).await?;
    info!("Received block from slot:{}", slot);
    ensure!(
        block.data.message.slot == state.data.slot,
        anyhow::anyhow!("Slot mismatch")
    );

    ensure!(block.data.message.state_root == state.data.state_root());
    let mut store = get_forkchoice_store(state.data, block.data.message, db)?;

    let time = network_spec().genesis.genesis_time + SECONDS_PER_SLOT * (slot + 1);
    on_tick(&mut store, time)?;
    info!("Initial Sync complete");

    Ok(())
}

/// Fetch initial state from trusted RPC
pub async fn get_state(
    rpc: &str,
    slot: u64,
) -> anyhow::Result<BeaconVersionedResponse<BeaconState>> {
    let state = reqwest::get(format!("{rpc}/eth/v2/debug/beacon/states/{slot}"))
        .await?
        .json::<BeaconVersionedResponse<BeaconState>>()
        .await?;

    info!("Fetched State Root:{}", state.data.state_root());

    Ok(state)
}

/// Fetch initial block from trusted RPC
pub async fn fetch_finalized_block(
    rpc: &str,
) -> anyhow::Result<BeaconVersionedResponse<SignedBeaconBlock>> {
    let block = reqwest::get(format!("{rpc}/eth/v2/beacon/blocks/finalized"))
        .await?
        .json::<BeaconVersionedResponse<SignedBeaconBlock>>()
        .await?;

    info!("Fetched Block Root:{}", block.data.message.block_root());

    Ok(block)
}

// fetch blobs from trusted RPC
pub async fn fetch_blobs(rpc: &str, store: ReamDB, beacon_block_root: B256) -> anyhow::Result<()> {
    let blob_sidecar = reqwest::get(format!(
        "{rpc}/eth/v1/beacon/blob_sidecars/{beacon_block_root}"
    ))
    .await?
    .json::<BeaconVersionedResponse<Vec<BlobSidecar>>>()
    .await?;

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
