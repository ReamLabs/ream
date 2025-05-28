use std::env;

use clap::Parser;
use ream::cli::{
    Cli, Commands, beacon_node::BeaconNodeConfig, validator_node::ValidatorNodeConfig,
};
use ream_checkpoint_sync::initialize_db_from_checkpoint;
use ream_consensus::constants::set_genesis_validator_root;
use ream_executor::ReamExecutor;
use ream_manager::service::ManagerService;
use ream_network_spec::networks::set_network_spec;
use ream_rpc::{config::RpcServerConfig, start_server};
use ream_storage::{
    db::{ReamDB, reset_db},
    dir::setup_data_dir,
};
use ream_validator::beacon_api_client::BeaconApiClient;
use tracing::info;
use tracing_subscriber::EnvFilter;

pub const APP_NAME: &str = "ream";

#[tokio::main]
async fn main() {
    // Set the default log level to `info` if not set
    let rust_log = env::var(EnvFilter::DEFAULT_ENV).unwrap_or_default();
    let env_filter = match rust_log.is_empty() {
        true => EnvFilter::builder().parse_lossy("info"),
        false => EnvFilter::builder().parse_lossy(rust_log),
    };

    tracing_subscriber::fmt().with_env_filter(env_filter).init();

    let cli = Cli::parse();

    let async_executor = ReamExecutor::new().expect("unable to create executor");

    let main_executor = ReamExecutor::new().expect("unable to create executor");

    match cli.command {
        Commands::BeaconNode(config) => {
            run_beacon_node(config, async_executor, main_executor).await
        }
        Commands::ValidatorNode(config) => run_validator_node(config).await,
    }
}

pub async fn run_beacon_node(
    config: BeaconNodeConfig,
    async_executor: ReamExecutor,
    main_executor: ReamExecutor,
) {
    info!("starting up beacon node...");

    set_network_spec(config.network.clone());

    let ream_dir = setup_data_dir(APP_NAME, config.data_dir.clone(), config.ephemeral)
        .expect("Unable to initialize database directory");

    if config.purge_db {
        reset_db(ream_dir.clone()).expect("Unable to delete database");
    }

    let ream_db = ReamDB::new(ream_dir).expect("unable to init Ream Database");

    info!("ream database initialized ");

    initialize_db_from_checkpoint(ream_db.clone(), config.checkpoint_sync_url.clone())
        .await
        .expect("Unable to initialize database from checkpoint");

    info!("Database Initialization completed");

    set_genesis_validator_root(
        ream_db
            .beacon_state_provider()
            .first()
            .expect("Failed to access beacon state provider")
            .expect("No beacon state found")
            .genesis_validators_root,
    );

    let server_config = RpcServerConfig::new(
        config.http_address,
        config.http_port,
        config.http_allow_origin,
    );

    let network_manager = ManagerService::new(async_executor, config.into(), ream_db.clone())
        .await
        .expect("Failed to create manager service");

    let peer_table = network_manager.peer_table().clone();

    let network_future = main_executor.spawn(async move {
        network_manager.start().await;
    });

    let http_future = start_server(server_config, ream_db, peer_table);

    tokio::select! {
        _ = http_future => {
            info!("HTTP server stopped!");
        },
        _ = network_future => {
            info!("Network future completed!");
        },
    }
}

pub async fn run_validator_node(config: ValidatorNodeConfig) {
    info!("starting up validator node...");

    BeaconApiClient::new(config.beacon_api_endpoint, config.request_timeout)
        .expect("Failed to create beacon api client");
}
