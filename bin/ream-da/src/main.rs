use std::{path::Path, str::FromStr, sync::Arc};

use alloy_primitives::{B256, U256, address, aliases::B32};
use clap::Parser;
use ream_consensus_misc::constants::beacon::set_genesis_validator_root;
use ream_da_beacon::DaConsensusClient;
use ream_da_errors::{DaError, DaResult};
use ream_da_networking::{DaNetworkService, blob_schedule::set_blob_schedule};
use ream_da_storage::DaStore;
use ream_executor::ReamExecutor;
use ream_network_spec::networks::{
    BeaconNetworkSpec, DEV, HOODI, MAINNET, Network, SEPOLIA, beacon_network_spec,
    set_beacon_network_spec,
};
use reqwest::Url;
use tracing::info;
use tracing_subscriber::{EnvFilter, fmt};

mod cli;
mod node;

use cli::Cli;
use node::DaNode;

#[tokio::main]
async fn main() -> DaResult<()> {
    fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,ream_da=debug")),
        )
        .init();

    let cli = Cli::parse();

    info!(
        beacon_url = %cli.beacon_url,
        network = %cli.network,
        "Starting ream-da node"
    );

    let beacon_url =
        Url::parse(&cli.beacon_url).map_err(|e| DaError::InvalidBeaconUrl(e.to_string()))?;

    let network_spec = match cli.network.as_str() {
        "mainnet" => MAINNET.clone(),
        "sepolia" => SEPOLIA.clone(),
        "hoodi" => HOODI.clone(),
        "dev" => DEV.clone(),
        _ => {
            info!(
                "Custom network '{}', fetching spec from beacon API",
                cli.network
            );
            fetch_network_spec(&beacon_url).await?
        }
    };
    set_beacon_network_spec(network_spec);

    let store = Arc::new(DaStore::new(cli.data_dir.clone())?);

    let (finalized_root, finalized_epoch) = fetch_finalized_checkpoint(&beacon_url)
        .await
        .unwrap_or((B256::ZERO, 0));
    let genesis_validators_root =
        resolve_genesis_validators_root(cli.genesis_validators_root, &beacon_url, &cli.data_dir)
            .await?;
    set_genesis_validator_root(genesis_validators_root);

    let blob_schedule = fetch_blob_schedule(&beacon_url).await?;
    set_blob_schedule(blob_schedule);

    let fork_digest = fetch_fork_digest(&beacon_url).await?;

    let consensus = DaConsensusClient::new(beacon_url)?;

    let bootnodes = cli
        .bootnodes
        .to_enrs_beacon(beacon_network_spec().network.clone());

    let static_peers = cli.static_peers.clone();

    let executor = ReamExecutor::new()
        .map_err(|e| DaError::Internal(format!("Failed to create executor: {}", e)))?;

    info!(finalized_epoch, %finalized_root, "Fetched initial finalized checkpoint");

    let network = DaNetworkService::new(
        executor.clone(),
        cli.p2p_host,
        cli.p2p_port,
        cli.discovery_port,
        bootnodes,
        static_peers,
        cli.data_dir.clone(),
        store.clone(),
        cli.disable_discovery,
        finalized_root,
        finalized_epoch,
        fork_digest,
    )
    .await?;

    info!(
        p2p_host = %cli.p2p_host,
        p2p_port = cli.p2p_port,
        discovery_port = cli.discovery_port,
        "P2P network initialized"
    );

    let node = DaNode::new(store, consensus, network);

    info!("ream-da node running — custodying 128 columns");

    node.run().await
}

async fn resolve_genesis_validators_root(
    cli_value: Option<B256>,
    beacon_url: &Url,
    data_dir: &Path,
) -> DaResult<B256> {
    if let Some(root) = cli_value {
        info!("Using genesis validators root from CLI flag");
        return Ok(root);
    }

    let cache_path = data_dir.join("genesis_validators_root");
    if cache_path.exists() {
        let hex = std::fs::read_to_string(&cache_path)
            .map_err(|e| DaError::Internal(format!("Failed to read genesis cache: {e}")))?;
        let root = hex
            .trim()
            .parse::<B256>()
            .map_err(|e| DaError::Internal(format!("Invalid cached genesis root: {e}")))?;
        info!(%root, "Using cached genesis validators root");
        return Ok(root);
    }

    info!("Fetching genesis validators root from beacon API");
    let root = fetch_genesis_validators_root(beacon_url).await?;

    std::fs::write(&cache_path, root.to_string())
        .map_err(|e| DaError::Internal(format!("Failed to cache genesis root: {e}")))?;

    info!(%root, "Genesis validators root fetched and cached");
    Ok(root)
}

async fn fetch_genesis_validators_root(beacon_url: &Url) -> DaResult<B256> {
    let url = beacon_url
        .join("/eth/v1/beacon/genesis")
        .map_err(|e| DaError::InvalidBeaconUrl(e.to_string()))?;

    let response = reqwest::get(url)
        .await
        .map_err(|e| DaError::EventStreamFailed(format!("Failed to fetch genesis: {e}")))?;

    let body: serde_json::Value = response.json().await.map_err(|e| {
        DaError::EventStreamFailed(format!("Failed to parse genesis response: {e}"))
    })?;

    let root_str = body["data"]["genesis_validators_root"]
        .as_str()
        .ok_or_else(|| DaError::EventStreamFailed("Missing genesis_validators_root".to_string()))?;

    root_str
        .parse::<B256>()
        .map_err(|e| DaError::EventStreamFailed(format!("Invalid genesis_validators_root: {e}")))
}

async fn fetch_network_spec(beacon_url: &Url) -> DaResult<Arc<BeaconNetworkSpec>> {
    let url = beacon_url
        .join("/eth/v1/config/spec")
        .map_err(|e| DaError::InvalidBeaconUrl(e.to_string()))?;

    let response = reqwest::get(url)
        .await
        .map_err(|e| DaError::EventStreamFailed(format!("Failed to fetch config spec: {e}")))?;

    let body: serde_json::Value = response
        .json()
        .await
        .map_err(|e| DaError::EventStreamFailed(format!("Failed to parse config spec: {e}")))?;

    let data = &body["data"];

    let parse_b32 = |key: &str| -> DaResult<B32> {
        data[key]
            .as_str()
            .ok_or_else(|| DaError::Internal(format!("Missing field: {key}")))?
            .parse::<B32>()
            .map_err(|e| DaError::Internal(format!("Invalid {key}: {e}")))
    };

    let parse_u64 = |key: &str| -> DaResult<u64> {
        data[key]
            .as_str()
            .ok_or_else(|| DaError::Internal(format!("Missing field: {key}")))?
            .parse::<u64>()
            .map_err(|e| DaError::Internal(format!("Invalid {key}: {e}")))
    };

    Ok(Arc::new(BeaconNetworkSpec {
        preset_base: "minimal".to_string(),
        network: Network::Custom(beacon_url.host_str().unwrap_or_default().to_string()),
        terminal_total_difficulty: U256::from_str("58750000000000000000000")
            .expect("Could not get U256"),
        terminal_block_hash: B256::ZERO,
        terminal_block_hash_activation_epoch: u64::MAX,
        min_genesis_active_validator_count: parse_u64("MIN_GENESIS_ACTIVE_VALIDATOR_COUNT")?,
        min_genesis_time: parse_u64("MIN_GENESIS_TIME")?,
        genesis_fork_version: parse_b32("GENESIS_FORK_VERSION")?,
        genesis_delay: parse_u64("GENESIS_DELAY")?,
        altair_fork_version: parse_b32("ALTAIR_FORK_VERSION")?,
        altair_fork_epoch: parse_u64("ALTAIR_FORK_EPOCH")?,
        bellatrix_fork_version: parse_b32("BELLATRIX_FORK_VERSION")?,
        bellatrix_fork_epoch: parse_u64("BELLATRIX_FORK_EPOCH")?,
        capella_fork_version: parse_b32("CAPELLA_FORK_VERSION")?,
        capella_fork_epoch: parse_u64("CAPELLA_FORK_EPOCH")?,
        deneb_fork_version: parse_b32("DENEB_FORK_VERSION")?,
        deneb_fork_epoch: parse_u64("DENEB_FORK_EPOCH")?,
        electra_fork_version: parse_b32("ELECTRA_FORK_VERSION")?,
        electra_fork_epoch: parse_u64("ELECTRA_FORK_EPOCH")?,
        fulu_fork_version: parse_b32("FULU_FORK_VERSION")?,
        fulu_fork_epoch: parse_u64("FULU_FORK_EPOCH")?,
        seconds_per_slot: parse_u64("SECONDS_PER_SLOT")?,
        seconds_per_eth1_block: 14,
        min_validator_withdrawability_delay: parse_u64("MIN_VALIDATOR_WITHDRAWABILITY_DELAY")?,
        shard_committee_period: parse_u64("SHARD_COMMITTEE_PERIOD")?,
        eth1_follow_distance: parse_u64("ETH1_FOLLOW_DISTANCE")?,
        inactivity_score_bias: parse_u64("INACTIVITY_SCORE_BIAS")?,
        inactivity_score_recovery_rate: parse_u64("INACTIVITY_SCORE_RECOVERY_RATE")?,
        ejection_balance: parse_u64("EJECTION_BALANCE")?,
        min_per_epoch_churn_limit: parse_u64("MIN_PER_EPOCH_CHURN_LIMIT")?,
        churn_limit_quotient: parse_u64("CHURN_LIMIT_QUOTIENT")?,
        max_per_epoch_activation_churn_limit: parse_u64("MAX_PER_EPOCH_ACTIVATION_CHURN_LIMIT")?,
        proposer_score_boost: parse_u64("PROPOSER_SCORE_BOOST")?,
        reorg_head_weight_threshold: parse_u64("REORG_HEAD_WEIGHT_THRESHOLD")?,
        reorg_parent_weight_threshold: parse_u64("REORG_PARENT_WEIGHT_THRESHOLD")?,
        reorg_max_epochs_since_finalization: parse_u64("REORG_MAX_EPOCHS_SINCE_FINALIZATION")?,
        deposit_chain_id: 1,
        deposit_network_id: 1,
        deposit_contract_address: address!("0x00000000219ab540356cBB839Cbe05303d7705Fa"),
        max_payload_size: 10485760,
        max_request_blocks: parse_u64("MAX_REQUEST_BLOCKS")?,
        epochs_per_subnet_subscription: parse_u64("EPOCHS_PER_SUBNET_SUBSCRIPTION")?,
        min_epochs_for_block_requests: parse_u64("MIN_EPOCHS_FOR_BLOCK_REQUESTS")?,
        ttfb_timeout: 5,
        resp_timeout: 10,
        attestation_propagation_slot_range: parse_u64("ATTESTATION_PROPAGATION_SLOT_RANGE")?,
        maximum_gossip_clock_disparity: parse_u64("MAXIMUM_GOSSIP_CLOCK_DISPARITY")?,
        message_domain_invalid_snappy: parse_b32("MESSAGE_DOMAIN_INVALID_SNAPPY")?,
        message_domain_valid_snappy: parse_b32("MESSAGE_DOMAIN_VALID_SNAPPY")?,
        subnets_per_node: parse_u64("SUBNETS_PER_NODE")?,
        attestation_subnet_count: parse_u64("ATTESTATION_SUBNET_COUNT")?,
        attestation_subnet_extra_bits: parse_u64("ATTESTATION_SUBNET_EXTRA_BITS")?,
        attestation_subnet_prefix_bits: parse_u64("ATTESTATION_SUBNET_PREFIX_BITS")?,
        max_request_blocks_deneb: parse_u64("MAX_REQUEST_BLOCKS_DENEB")?,
        max_request_blob_sidecars: parse_u64("MAX_REQUEST_BLOB_SIDECARS")?,
        min_epochs_for_blob_sidecars_requests: parse_u64("MIN_EPOCHS_FOR_BLOB_SIDECARS_REQUESTS")?,
        blob_sidecar_subnet_count: parse_u64("BLOB_SIDECAR_SUBNET_COUNT")?,
        min_per_epoch_churn_limit_electra: parse_u64("MIN_PER_EPOCH_CHURN_LIMIT_ELECTRA")?,
        max_per_epoch_activation_exit_churn_limit: parse_u64(
            "MAX_PER_EPOCH_ACTIVATION_EXIT_CHURN_LIMIT",
        )?,
        blob_sidecar_subnet_count_electra: parse_u64("BLOB_SIDECAR_SUBNET_COUNT_ELECTRA")?,
        max_blobs_per_block_electra: parse_u64("MAX_BLOBS_PER_BLOCK_ELECTRA")?,
        max_request_blob_sidecars_electra: parse_u64("MAX_REQUEST_BLOB_SIDECARS_ELECTRA")?,
    }))
}

async fn fetch_blob_schedule(beacon_url: &Url) -> DaResult<Vec<(u64, u64)>> {
    let url = beacon_url
        .join("/eth/v1/config/spec")
        .map_err(|e| DaError::InvalidBeaconUrl(e.to_string()))?;

    let response = reqwest::get(url)
        .await
        .map_err(|e| DaError::EventStreamFailed(format!("Failed to fetch spec: {e}")))?;

    let body: serde_json::Value = response
        .json()
        .await
        .map_err(|e| DaError::EventStreamFailed(format!("Failed to parse spec: {e}")))?;

    let schedule = body["data"]["BLOB_SCHEDULE"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .filter_map(|entry| {
            let epoch = entry["EPOCH"].as_str()?.parse::<u64>().ok()?;
            let max_blobs = entry["MAX_BLOBS_PER_BLOCK"].as_str()?.parse::<u64>().ok()?;
            Some((epoch, max_blobs))
        })
        .collect();

    Ok(schedule)
}

async fn fetch_finalized_checkpoint(beacon_url: &Url) -> DaResult<(B256, u64)> {
    let url = beacon_url
        .join("/eth/v1/beacon/states/head/finality_checkpoints")
        .map_err(|e| DaError::InvalidBeaconUrl(e.to_string()))?;

    let response = reqwest::get(url)
        .await
        .map_err(|e| DaError::EventStreamFailed(format!("Failed to fetch finality: {e}")))?;

    let body: serde_json::Value = response
        .json()
        .await
        .map_err(|e| DaError::EventStreamFailed(format!("Failed to parse finality: {e}")))?;

    let root = body["data"]["finalized"]["root"]
        .as_str()
        .unwrap_or_default()
        .parse::<B256>()
        .unwrap_or_default();

    let epoch = body["data"]["finalized"]["epoch"]
        .as_str()
        .unwrap_or("0")
        .parse::<u64>()
        .unwrap_or(0);

    Ok((root, epoch))
}

async fn fetch_fork_digest(beacon_url: &Url) -> DaResult<alloy_primitives::aliases::B32> {
    let identity_url = beacon_url
        .join("/eth/v1/node/identity")
        .map_err(|e| DaError::InvalidBeaconUrl(e.to_string()))?;

    let identity_response = reqwest::get(identity_url)
        .await
        .map_err(|e| DaError::EventStreamFailed(format!("Failed to fetch identity: {e}")))?;

    let identity_body: serde_json::Value = identity_response
        .json()
        .await
        .map_err(|e| DaError::EventStreamFailed(format!("Failed to parse identity: {e}")))?;

    let enr_str = identity_body["data"]["enr"]
        .as_str()
        .ok_or_else(|| DaError::EventStreamFailed("Missing enr".to_string()))?;

    let enr: discv5::Enr = enr_str
        .parse()
        .map_err(|e| DaError::EventStreamFailed(format!("Failed to parse ENR: {e}")))?;

    let eth2_bytes = enr
        .get_raw_rlp("eth2")
        .ok_or_else(|| DaError::EventStreamFailed("Missing eth2 field in ENR".to_string()))?;

    if eth2_bytes.len() < 5 {
        return Err(DaError::EventStreamFailed(
            "eth2 field too short".to_string(),
        ));
    }

    let fork_digest = alloy_primitives::aliases::B32::from_slice(&eth2_bytes[1..5]);
    info!("Fetched fork digest from beacon ENR: {fork_digest:#x}");
    Ok(fork_digest)
}
