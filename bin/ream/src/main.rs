use std::{env, process, sync::Arc};

use clap::Parser;
use ream::cli::{
    Cli, Commands,
    account_manager::AccountManagerConfig,
    beacon_node::BeaconNodeConfig,
    import_keystores::{load_keystore_directory, load_password_file, process_password},
    validator_node::ValidatorNodeConfig,
};
use ream_checkpoint_sync::initialize_db_from_checkpoint;
use ream_consensus::constants::set_genesis_validator_root;
use ream_executor::ReamExecutor;
use ream_manager::service::ManagerService;
use ream_network_spec::networks::set_network_spec;
use ream_operation_pool::OperationPool;
use ream_rpc::{config::RpcServerConfig, start_server};
use ream_storage::{
    db::{ReamDB, reset_db},
    dir::setup_data_dir,
    tables::Table,
};
use ream_validator::validator::ValidatorService;
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
            run_beacon_node(*config, async_executor, main_executor).await
        }
        Commands::ValidatorNode(config) => run_validator_node(*config, async_executor).await,
        Commands::AccountManager(config) => run_account_manager(*config).await,
    }
    process::exit(0);
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

    let network_manager = ManagerService::new(
        async_executor,
        config.into(),
        ream_db.clone(),
        ream_dir,
        operation_pool.clone(),
    )
    .await
    .expect("Failed to create manager service");

    let network_state = network_manager.network_state.clone();

    let network_future = main_executor.spawn(async move {
        network_manager.start().await;
    });

    let http_future = start_server(server_config, ream_db, network_state, operation_pool);

    tokio::select! {
        _ = http_future => {
            info!("HTTP server stopped!");
        },
        _ = network_future => {
            info!("Network future completed!");
        },
    }
}

pub async fn run_validator_node(config: ValidatorNodeConfig, async_executor: ReamExecutor) {
    info!("starting up validator node...");

    set_network_spec(config.network.clone());

    let password = process_password({
        if let Some(ref password_file) = config.password_file {
            load_password_file(password_file).expect("Failed to read password from password file")
        } else if let Some(password_str) = config.password {
            password_str
        } else {
            panic!("Expected either password or password-file to be set")
        }
    });

    let key_stores = load_keystore_directory(&config.import_keystores)
        .expect("Failed to load keystore directory")
        .into_iter()
        .map(|encrypted_keystore| {
            encrypted_keystore
                .decrypt(password.as_bytes())
                .expect("Could not decrypt a keystore")
        })
        .collect::<Vec<_>>();

    let validator_service = ValidatorService::new(
        key_stores,
        config.suggested_fee_recipient,
        config.beacon_api_endpoint,
        config.request_timeout,
        async_executor,
    )
    .expect("Failed to create validator service");

    validator_service.start().await;
}

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
