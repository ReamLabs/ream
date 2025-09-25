use std::{
    env, fs,
    net::SocketAddr,
    path::{Path, PathBuf},
    process,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use alloy_primitives::{FixedBytes, hex};
use bip39::Mnemonic;
use clap::Parser;
use libp2p_identity::secp256k1;
use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;
use ream::cli::{
    Cli, Commands,
    account_manager::AccountManagerConfig,
    beacon_node::BeaconNodeConfig,
    generate_private_key::GeneratePrivateKeyConfig,
    import_keystores::{load_keystore_directory, load_password_from_config, process_password},
    lean_node::LeanNodeConfig,
    validator_node::ValidatorNodeConfig,
    voluntary_exit::VoluntaryExitConfig,
};
use ream_account_manager::{message_types::MessageType, seed::derive_seed_with_user_input};
use ream_api_types_beacon::id::ValidatorID;
use ream_api_types_common::id::ID;
use ream_chain_lean::{
    genesis as lean_genesis, lean_chain::LeanChain, messages::LeanChainServiceMessage,
    p2p_request::LeanP2PRequest, service::LeanChainService,
};
use ream_checkpoint_sync::initialize_db_from_checkpoint;
use ream_consensus_lean::block::SignedBlock;
use ream_consensus_misc::{
    constants::beacon::set_genesis_validator_root, misc::compute_epoch_at_slot,
};
use ream_executor::ReamExecutor;
use ream_keystore::keystore::EncryptedKeystore;
use ream_network_manager::service::NetworkManagerService;
use ream_network_spec::networks::{
    beacon_network_spec, set_beacon_network_spec, set_lean_network_spec,
};
use ream_operation_pool::OperationPool;
use ream_p2p::{
    gossipsub::lean::{
        configurations::LeanGossipsubConfig,
        topics::{LeanGossipTopic, LeanGossipTopicKind},
    },
    network::lean::{LeanNetworkConfig, LeanNetworkService},
};
use ream_post_quantum_crypto::hashsig::private_key::PrivateKey as HashSigPrivateKey;
use ream_rpc_beacon::{config::RpcServerConfig, start_server};
use ream_rpc_lean::{config::LeanRpcServerConfig, start_lean_server};
use ream_storage::{
    db::{ReamDB, reset_db},
    dir::setup_data_dir,
    tables::table::Table,
};
use ream_sync::rwlock::Writer;
use ream_validator_beacon::{
    beacon_api_client::BeaconApiClient, validator::ValidatorService,
    voluntary_exit::process_voluntary_exit,
};
use ream_validator_lean::{
    registry::load_validator_registry, service::ValidatorService as LeanValidatorService,
};
use tokio::{sync::mpsc, time::Instant};
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
    let ream_dir = setup_data_dir(APP_NAME, cli.data_dir.clone(), cli.ephemeral)
        .expect("Unable to initialize database directory");

    if cli.purge_db {
        reset_db(&ream_dir).expect("Unable to delete database");
    }

    let ream_db = ReamDB::new(ream_dir.clone()).expect("unable to init Ream Database");

    match cli.command {
        Commands::LeanNode(config) => {
            executor_clone.spawn(async move { run_lean_node(*config, executor, ream_db).await });
        }
        Commands::BeaconNode(config) => {
            executor_clone.spawn(async move { run_beacon_node(*config, executor, ream_db).await });
        }
        Commands::ValidatorNode(config) => {
            executor_clone.spawn(async move { run_validator_node(*config, executor).await });
        }
        Commands::AccountManager(config) => {
            executor_clone.spawn(async move { run_account_manager(*config, ream_dir).await });
        }
        Commands::VoluntaryExit(config) => {
            executor_clone.spawn(async move { run_voluntary_exit(*config).await });
        }
        Commands::GeneratePrivateKey(config) => {
            executor_clone.spawn(async move { run_generate_private_key(*config).await });
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
pub async fn run_lean_node(config: LeanNodeConfig, executor: ReamExecutor, ream_db: ReamDB) {
    info!("starting up lean node...");

    // Initialize prometheus metrics
    if config.enable_metrics {
        let address = SocketAddr::new(config.metrics_address, config.metrics_port);
        prometheus_exporter::start(address).expect("Failed to start prometheus exporter");
        info!(
            "Metrics started on {}:{}",
            config.metrics_address, config.metrics_port
        );
    }

    set_lean_network_spec(config.network);

    // Initialize the lean database
    let lean_db = ream_db
        .init_lean_db()
        .expect("unable to init Ream Lean Database");

    info!("ream lean database has been initialized");

    // Initialize the lean chain with genesis block and state.
    let (genesis_block, genesis_state) = lean_genesis::setup_genesis();
    let (lean_chain_writer, lean_chain_reader) = Writer::new(LeanChain::new(
        SignedBlock {
            message: genesis_block,
            signature: FixedBytes::default(),
        },
        genesis_state,
        lean_db,
    ));

    // Initialize the services that will run in the lean node.
    let (chain_sender, chain_receiver) = mpsc::unbounded_channel::<LeanChainServiceMessage>();
    let (outbound_p2p_sender, outbound_p2p_receiver) = mpsc::unbounded_channel::<LeanP2PRequest>();

    let chain_service = LeanChainService::new(
        lean_chain_writer,
        chain_receiver,
        chain_sender.clone(),
        outbound_p2p_sender,
    )
    .await;

    let fork = "devnet0".to_string();
    let topics: Vec<LeanGossipTopic> = vec![
        LeanGossipTopic {
            fork: fork.clone(),
            kind: LeanGossipTopicKind::Block,
        },
        LeanGossipTopic {
            fork,
            kind: LeanGossipTopicKind::Vote,
        },
    ];

    let gossipsub_config = LeanGossipsubConfig {
        topics,
        ..Default::default()
    };

    let mut network_service = LeanNetworkService::new(
        Arc::new(LeanNetworkConfig {
            gossipsub_config,
            socket_address: config.socket_address,
            socket_port: config.socket_port,
            private_key_path: config.private_key_path,
        }),
        lean_chain_reader.clone(),
        executor.clone(),
        chain_sender.clone(),
        outbound_p2p_receiver,
    )
    .await
    .expect("Failed to create network service");

    let peer_table = network_service.peer_table();

    let keystores = load_validator_registry(&config.validator_registry_path, &config.node_id)
        .expect("Failed to load validator registry");
    let validator_service =
        LeanValidatorService::new(lean_chain_reader.clone(), keystores, chain_sender).await;

    let server_config = LeanRpcServerConfig::new(
        config.http_address,
        config.http_port,
        config.http_allow_origin,
    );

    // Start the services concurrently.
    let chain_future = executor.spawn(async move {
        if let Err(err) = chain_service.start().await {
            panic!("Chain service exited with error: {err:?}");
        }
    });
    let network_future = executor.spawn(async move {
        if let Err(err) = network_service.start(config.bootnodes).await {
            panic!("Network service exited with error: {err:?}");
        }
    });
    let validator_future = executor.spawn(async move {
        if let Err(err) = validator_service.start().await {
            panic!("Validator service exited with error: {err:?}");
        }
    });
    let http_future = executor.spawn(async move {
        start_lean_server(server_config, lean_chain_reader, peer_table).await
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
        _ = http_future => {
            info!("RPC service has stopped unexpectedly");
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
pub async fn run_beacon_node(config: BeaconNodeConfig, executor: ReamExecutor, ream_db: ReamDB) {
    info!("starting up beacon node...");

    set_beacon_network_spec(config.network.clone());

    // Initialize the beacon database
    let beacon_db = ream_db
        .init_beacon_db()
        .expect("unable to init Ream Beacon Database");

    info!("ream beacon database has been initialized");

    let _is_ws_verified = initialize_db_from_checkpoint(
        beacon_db.clone(),
        config.checkpoint_sync_url.clone(),
        config.weak_subjectivity_checkpoint,
    )
    .await
    .expect("Unable to initialize database from checkpoint");

    info!("Database Initialization completed");

    let oldest_root = beacon_db
        .slot_index_provider()
        .get_oldest_root()
        .expect("Failed to access slot index provider")
        .expect("No oldest root found");
    set_genesis_validator_root(
        beacon_db
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
        beacon_db.clone(),
        beacon_db.data_dir.clone(),
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
            beacon_db,
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
pub async fn run_account_manager(config: AccountManagerConfig, ream_dir: PathBuf) {
    info!("Starting account manager...");

    info!(
        "Account manager configuration: lifetime={}, chunk_size={}, activation_epoch={}, num_active_epochs={}",
        config.lifetime, config.chunk_size, config.activation_epoch, config.num_active_epochs
    );

    // Get seed phrase or generate a new one
    let seed_phrase = config.seed_phrase.unwrap_or_else(|| {
        let mnemonic = Mnemonic::generate(24).expect("Failed to generate mnemonic");
        let seed_phrase = mnemonic.words().collect::<Vec<_>>().join(" ");
        info!("{}", "=".repeat(89));
        info!("Generated new seed phrase (KEEP SAFE): {seed_phrase}");
        info!("{}", "=".repeat(89));
        seed_phrase
    });

    // Create keystore directory as subdirectory of data directory
    let keystore_dir = match &config.keystore_path {
        Some(custom_path) => {
            let path = Path::new(custom_path);
            if path.is_absolute() {
                path.to_path_buf()
            } else {
                ream_dir.join(custom_path)
            }
        }
        None => ream_dir.join("keystores"),
    };

    if !keystore_dir.exists() {
        fs::create_dir_all(&keystore_dir).expect("Failed to create keystore directory");
        info!(
            "Created keystore directory: {path}",
            path = keystore_dir.display()
        );
    }

    // Measure key generation time
    let start_time = Instant::now();

    // Generate keys sequentially for each message type
    for (index, message_type) in MessageType::iter().enumerate() {
        info!(
            "Generating lean consensus validator keys for index {index}, message type: {message_type}..."
        );

        let seed = derive_seed_with_user_input(
            &seed_phrase,
            index as u32,
            config.passphrase.as_deref().unwrap_or(""),
        );

        let (public_key, _private_key) = HashSigPrivateKey::generate_key_pair(
            &mut <ChaCha20Rng as SeedableRng>::from_seed(seed),
            config.activation_epoch as usize,
            config.num_active_epochs as usize,
        );

        info!(
            "Public key for {message_type}: {}",
            // This should never panic
            serde_json::to_string_pretty(&public_key).expect("Failed to serialize public key")
        );

        // Create keystore file using Keystore
        let keystore = EncryptedKeystore::from_seed_phrase(
            &seed_phrase,
            config.lifetime,
            config.activation_epoch,
            Some(format!("Ream validator keystore for {message_type}")),
            Some(format!("m/44'/60'/0'/0/{index}")),
        );

        // Write keystore to file with enum name
        let filename = message_type.to_string();
        let keystore_file_path = keystore_dir.join(filename);
        let keystore_json =
            ::serde_json::to_string_pretty(&keystore).expect("Failed to serialize keystore");

        fs::write(&keystore_file_path, keystore_json).expect("Failed to write keystore file");

        info!("Keystore written to path: {}", keystore_file_path.display());
    }
    let duration = start_time.elapsed();
    info!("Key generation complete, took {:?}", duration);

    info!("Account manager completed successfully");

    process::exit(0);
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

/// Generates a new secp256k1 keypair and saves it to the specified path in hex encoding.
///
/// This allows the lean node to reuse the same network identity across restarts by loading
/// the saved key with the --private-key-path flag.
pub async fn run_generate_private_key(config: GeneratePrivateKeyConfig) {
    info!("Generating new secp256k1 private key...");

    assert!(
        !config.output_path.is_dir(),
        "Output path must point to a file, not a directory: {}",
        config.output_path.display()
    );

    if let Some(parent) = config.output_path.parent() {
        fs::create_dir_all(parent).expect("Failed to create parent directories");
    }

    fs::write(
        &config.output_path,
        hex::encode(secp256k1::Keypair::generate().secret().to_bytes()),
    )
    .expect("Failed to write keypair to file");

    info!(
        "secp256k1 private key generated successfully and saved to: {}",
        config.output_path.display()
    );

    process::exit(0);
}
