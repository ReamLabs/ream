pub mod checkpoint;
pub mod weak_subjectivity;

use std::{fs, path::Path};

use alloy_primitives::B256;
use anyhow::{anyhow, ensure};
use checkpoint::get_checkpoint_sync_sources;
use ream_consensus_beacon::{
    blob_sidecar::{BlobIdentifier, BlobSidecar},
    electra::{
        beacon_block::{BeaconBlock, SignedBeaconBlock},
        beacon_state::BeaconState,
    },
};
use ream_consensus_misc::checkpoint::Checkpoint;
use ream_execution_rpc_types::get_blobs::BlobAndProofV1;
use ream_fork_choice_beacon::{handlers::on_tick, store::get_forkchoice_store};
use ream_network_spec::networks::beacon_network_spec;
use ream_storage::{
    db::beacon::BeaconDB,
    tables::table::{CustomTable, REDBTable},
};
use reqwest::{
    Url,
    header::{ACCEPT, HeaderValue},
};
use serde::{Deserialize, Serialize};
use ssz::Decode;
use tracing::{info, warn};
use tree_hash::TreeHash;
use weak_subjectivity::{WeakSubjectivityState, verify_state_from_weak_subjectivity_checkpoint};

/// Entry point for checkpoint sync.
pub async fn initialize_db_from_checkpoint(
    db: BeaconDB,
    checkpoint_sync_url: Option<Url>,
    weak_subjectivity_checkpoint: Option<Checkpoint>,
) -> anyhow::Result<WeakSubjectivityState> {
    if db.is_initialized() {
        warn!("DB is already initialized. Skipping checkpoint sync.");

        let highest_root = db
            .slot_index_provider()
            .get_highest_root()?
            .expect("No highest root found");
        let state = db
            .state_provider()
            .get(highest_root)?
            .ok_or_else(|| anyhow!("Unable to fetch state"))?;

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

    let sources = get_checkpoint_sync_sources(checkpoint_sync_url);
    ensure!(
        !sources.is_empty(),
        "No checkpoint sync source available for network {:?}. Pass --checkpoint-sync-url \
         explicitly, use --genesis-state-path for a fresh local devnet, or use a network with \
         default checkpoint sync sources (mainnet, sepolia, hoodi).",
        beacon_network_spec().network
    );
    let checkpoint_sync_url = sources.into_iter().next().expect("checked non-empty above");
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

    let time = beacon_network_spec().min_genesis_time
        + beacon_network_spec().seconds_per_slot() * (slot + 1);
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

// Bootstrap the database directly from a local genesis state file (SSZ-encoded BeaconState),
/// skipping checkpoint sync entirely for local devnets (e.g. Kurtosis)
pub fn initialize_db_from_genesis_state(
    db: BeaconDB,
    genesis_state_path: &Path,
) -> anyhow::Result<()> {
    if db.is_initialized() {
        warn!("DB is already initialized. Skipping genesis bootstrap.");
        return Ok(());
    }

    info!(
        "Bootstrapping from local genesis state: {}",
        genesis_state_path.display()
    );

    let raw_bytes = fs::read(genesis_state_path).map_err(|err| {
        anyhow!(
            "Failed to read genesis state file {}: {err}",
            genesis_state_path.display()
        )
    })?;

    info!("Read {} bytes from genesis.ssz", raw_bytes.len());

    let genesis_state = BeaconState::from_ssz_bytes(&raw_bytes)
        .map_err(|err| anyhow!("Unable to decode genesis state from ssz bytes: {err:?}"))?;

    let genesis_block = BeaconBlock {
        slot: genesis_state.slot,
        proposer_index: 0,
        parent_root: B256::ZERO,
        state_root: genesis_state.tree_hash_root(),
        ..Default::default()
    };

    info!(
        "genesis_time={}, genesis_validators_root={}",
        genesis_state.genesis_time, genesis_state.genesis_validators_root
    );

    let mut store = get_forkchoice_store(genesis_state.clone(), genesis_block, db)?;

    let time = genesis_state.genesis_time
        + beacon_network_spec().seconds_per_slot() * (genesis_state.slot + 1);
    on_tick(&mut store, time)?;

    info!("Genesis bootstrap complete");
    Ok(())
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
    store: BeaconDB,
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
