use std::{
    env,
    ops::Deref,
    process,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use clap::Parser;
use ream::cli::{
    Cli, Commands,
    account_manager::AccountManagerConfig,
    beacon_node::BeaconNodeConfig,
    import_keystores::{load_keystore_directory, load_password_from_config, process_password},
    lean_node::LeanNodeConfig,
    validator_node::ValidatorNodeConfig,
    voluntary_exit::VoluntaryExitConfig,
};
use ream_beacon_api_types::id::{ID, ValidatorID};
use ream_chain_lean::{
    genesis as lean_genesis,
    lean_chain::LeanChain,
    service::{LeanChainService, LeanChainServiceMessage},
};
use ream_checkpoint_sync::initialize_db_from_checkpoint;
use ream_consensus_misc::{
    constants::beacon::set_genesis_validator_root, misc::compute_epoch_at_slot,
};
use ream_executor::ReamExecutor;
use ream_network_manager::service::NetworkManagerService;
use ream_network_spec::networks::{
    beacon_network_spec, set_beacon_network_spec, set_lean_network_spec,
};
use ream_operation_pool::OperationPool;
use ream_p2p::{
    gossipsub::lean::configurations::LeanGossipsubConfig,
    network::lean::{LeanNetworkConfig, LeanNetworkService},
};
use ream_rpc_beacon::{config::RpcServerConfig, start_server};
use ream_storage::{
    db::{ReamDB, reset_db},
    dir::setup_data_dir,
    tables::Table,
};
use ream_validator_beacon::{
    beacon_api_client::BeaconApiClient, validator::ValidatorService,
    voluntary_exit::process_voluntary_exit,
};
use ream_validator_lean::service::ValidatorService as LeanValidatorService;
use tokio::sync::{RwLock, mpsc};
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

pub const APP_NAME: &str = "ream";

/// Entry point for the Ream client. Initializes logging, parses CLI arguments, and runs the
/// appropriate node type (beacon node, validator node, or account manager) based on the command
/// line arguments. Handles graceful shutdown on Ctrl-C.
fn main() {
    // Set the default log level to `info` if not set
    let rust_log = env::var(EnvFilter::DEFAULT_ENV).unwrap_or_default();
    let env_filter = match rust_log.is_empty() {
        true => EnvFilter::builder().parse_lossy("info,actix_server=warn,discv5=error"),
        false => EnvFilter::builder().parse_lossy(rust_log),
    };

    tracing_subscriber::fmt().with_env_filter(env_filter).init();

    let cli = Cli::parse();

    let executor = ReamExecutor::new().expect("unable to create executor");
    let executor_clone = executor.clone();

    match cli.command {
        Commands::LeanNode(config) => {
            executor_clone.spawn(async move { run_lean_node(*config, executor).await });
        }
        Commands::BeaconNode(config) => {
            executor_clone.spawn(async move { run_beacon_node(*config, executor).await });
        }
        Commands::ValidatorNode(config) => {
            executor_clone.spawn(async move { run_validator_node(*config, executor).await });
        }
        Commands::AccountManager(config) => {
            executor_clone.spawn(async move { run_account_manager(*config).await });
        }
        Commands::VoluntaryExit(config) => {
            executor_clone.spawn(async move { run_voluntary_exit(*config).await });
        }
    }

    executor_clone.runtime().block_on(async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to pause until ctrl-c");
        info!("Ctrl-C received, shutting down...");
        executor_clone.shutdown_signal();
    });

    executor_clone.shutdown_runtime();

    process::exit(0);
}

/// Runs the lean node.
///
/// A lean node runs several services with different responsibilities.
/// Refer to each service's documentation for more details.
///
/// A lean node has one shared state, `LeanChain` (wrapped with synchronization primitives), which
/// is used by all services.
///
/// Besides the shared state, each service holds the channels to communicate with each other.
pub async fn run_lean_node(config: LeanNodeConfig, executor: ReamExecutor) {
    info!("starting up lean node...");

    // Hack: It is bothersome to modify the spec every time we run the lean node.
    // Set genesis time to a future time if it is in the past.
    // FIXME: Add a script to generate the YAML config file.
    let network = {
        let current_timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("System time is before UNIX epoch")
            .as_secs();

        if config.network.genesis_time < current_timestamp {
            let mut network = config.network.deref().clone();
            network.genesis_time = current_timestamp + 3; // Set genesis time to 3 seconds in the future.
            Arc::new(network)
        } else {
            config.network.clone()
        }
    };

    set_lean_network_spec(network);

    // Initialize the lean chain with genesis block and state.
    let (genesis_block, genesis_state) = lean_genesis::setup_genesis();
    let lean_chain = Arc::new(RwLock::new(LeanChain::new(genesis_block, genesis_state)));

    // Initialize the services that will run in the lean node.
    let (chain_sender, chain_receiver) = mpsc::unbounded_channel::<LeanChainServiceMessage>();

    // TODO 1: Load keystores from the config.
    // TODO 2: Add RPC service for lean node.
    let chain_service =
        LeanChainService::new(lean_chain.clone(), chain_receiver, chain_sender.clone()).await;
    let network_service = LeanNetworkService::new(
        Arc::new(LeanNetworkConfig {
            gossipsub_config: LeanGossipsubConfig::default(),
        }),
        lean_chain.clone(),
        executor.clone(),
    )
    .await
    .expect("Failed to create network service");

    let validator_service =
        LeanValidatorService::new(lean_chain.clone(), Vec::new(), chain_sender).await;

    // Start the services concurrently.
    let chain_future = executor.spawn(async move {
        if let Err(err) = chain_service.start().await {
            panic!("Chain service exited with error: {err}");
        }
    });
    let network_future = executor.spawn(async move {
        if let Err(err) = network_service.start().await {
            panic!("Network service exited with error: {err}");
        }
    });
    let validator_future = executor.spawn(async move {
        if let Err(err) = validator_service.start().await {
            panic!("Validator service exited with error: {err}");
        }
    });

    tokio::select! {
        _ = chain_future => {
            info!("Chain service has stopped unexpectedly");
        }
        _ = network_future => {
            info!("Network service has stopped unexpectedly");
        }
        _ = validator_future => {
            info!("Validator service has stopped unexpectedly");
        }
    }
}

/// Runs the beacon node.
///
/// This function initializes the beacon node by setting up the network specification,
/// creating a Ream database, and initializing the database from a checkpoint.
///
/// At the end of setup, it starts 2 services:
/// 1. The HTTP server that serves Beacon API, Engine API.
/// 2. The P2P network that handles peer discovery (discv5), gossiping (gossipsub) and Req/Resp API.
pub async fn run_beacon_node(config: BeaconNodeConfig, executor: ReamExecutor) {
    info!("starting up beacon node...");

    set_beacon_network_spec(config.network.clone());

    let ream_dir = setup_data_dir(APP_NAME, config.data_dir.clone(), config.ephemeral)
        .expect("Unable to initialize database directory");

    if config.purge_db {
        reset_db(ream_dir.clone()).expect("Unable to delete database");
    }

    let ream_db = ReamDB::new(ream_dir.clone()).expect("unable to init Ream Database");

    info!("ream database initialized ");

    let _is_ws_verified = initialize_db_from_checkpoint(
        ream_db.clone(),
        config.checkpoint_sync_url.clone(),
        config.weak_subjectivity_checkpoint,
    )
    .await
    .expect("Unable to initialize database from checkpoint");

    info!("Database Initialization completed");

    let oldest_root = ream_db
        .slot_index_provider()
        .get_oldest_root()
        .expect("Failed to access slot index provider")
        .expect("No oldest root found");
    set_genesis_validator_root(
        ream_db
            .beacon_state_provider()
            .get(oldest_root)
            .expect("Failed to access beacon state provider")
            .expect("No beacon state found")
            .genesis_validators_root,
    );

    let operation_pool = Arc::new(OperationPool::default());

    let server_config = RpcServerConfig::new(
        config.http_address,
        config.http_port,
        config.http_allow_origin,
    );

    let network_manager = NetworkManagerService::new(
        executor.clone(),
        config.into(),
        ream_db.clone(),
        ream_dir,
        operation_pool.clone(),
    )
    .await
    .expect("Failed to create manager service");

    let network_state = network_manager.network_state.clone();

    let execution_engine = network_manager.beacon_chain.execution_engine.clone();

    let network_future = executor.spawn(async move {
        network_manager.start().await;
    });

    let http_future = executor.spawn(async move {
        start_server(
            server_config,
            ream_db,
            network_state,
            operation_pool,
            execution_engine,
        )
        .await
    });

    tokio::select! {
        _ = http_future => {
            info!("HTTP server stopped!");
        },
        _ = network_future => {
            info!("Network future completed!");
        },
    }
}

/// Runs the validator node.
///
/// This function initializes the validator node by setting up the network specification,
/// loading the keystores, and creating a validator service.
/// It also starts the validator service.
pub async fn run_validator_node(config: ValidatorNodeConfig, executor: ReamExecutor) {
    info!("starting up validator node...");

    set_beacon_network_spec(config.network.clone());

    let password = process_password(
        load_password_from_config(config.password_file.as_ref(), config.password)
            .expect("Failed to load password"),
    );

    let keystores = load_keystore_directory(&config.import_keystores)
        .expect("Failed to load keystore directory")
        .into_iter()
        .map(|encrypted_keystore| {
            encrypted_keystore
                .decrypt(password.as_bytes())
                .expect("Could not decrypt a keystore")
        })
        .collect::<Vec<_>>();

    let validator_service = ValidatorService::new(
        keystores,
        config.suggested_fee_recipient,
        config.beacon_api_endpoint,
        config.request_timeout,
        executor,
    )
    .expect("Failed to create validator service");

    validator_service.start().await;
}

/// Runs the account manager.
///
/// This function initializes the account manager by validating the configuration,
/// generating keys, and starting the account manager service.
pub async fn run_account_manager(mut config: AccountManagerConfig) {
    info!("starting up account manager...");

    // Validate the configuration
    config
        .validate()
        .expect("Invalid account manager configuration");

    info!(
        "Account manager configuration: lifetime={}, chunk_size={}",
        config.lifetime, config.chunk_size
    );

    let seed_phrase = config.get_seed_phrase();
    ream_account_manager::generate_keys(&seed_phrase);

    info!("Account manager completed successfully");
}

/// Runs the voluntary exit process.
///
/// This function initializes the voluntary exit process by setting up the network specification,
/// loading the keystores, creating a validator service, and processing the voluntary exit.
pub async fn run_voluntary_exit(config: VoluntaryExitConfig) {
    info!("Starting voluntary exit process...");

    set_beacon_network_spec(config.network.clone());

    let password = process_password(
        load_password_from_config(config.password_file.as_ref(), config.password)
            .expect("Failed to load password"),
    );

    let keystores = load_keystore_directory(&config.import_keystores)
        .expect("Failed to load keystore directory")
        .into_iter()
        .map(|encrypted_keystore| {
            encrypted_keystore
                .decrypt(password.as_bytes())
                .expect("Could not decrypt a keystore")
        })
        .collect::<Vec<_>>();

    let beacon_api_client =
        BeaconApiClient::new(config.beacon_api_endpoint, config.request_timeout)
            .expect("Failed to create beacon API client");

    let validator_info = beacon_api_client
        .get_state_validator(ID::Head, ValidatorID::Index(config.validator_index))
        .await
        .expect("Failed to get validator info");

    let keystore = keystores
        .iter()
        .find(|keystore| keystore.public_key == validator_info.data.validator.public_key)
        .expect("No keystore found for the specified validator index");

    let genesis = beacon_api_client
        .get_genesis()
        .await
        .expect("Failed to get genesis information");

    match process_voluntary_exit(
        &beacon_api_client,
        config.validator_index,
        get_current_epoch(genesis.data.genesis_time),
        &keystore.private_key,
        config.wait,
    )
    .await
    {
        Ok(()) => info!("Voluntary exit completed successfully"),
        Err(err) => error!("Voluntary exit failed: {err}"),
    }
}

/// Calculates the current epoch from genesis time
fn get_current_epoch(genesis_time: u64) -> u64 {
    compute_epoch_at_slot(
        SystemTime::now()
            .duration_since(UNIX_EPOCH + Duration::from_secs(genesis_time))
            .expect("System Time is before the genesis time")
            .as_secs()
            / beacon_network_spec().seconds_per_slot,
    )
}
