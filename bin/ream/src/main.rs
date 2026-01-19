use std::{
    env, fs,
    net::SocketAddr,
    path::{Path, PathBuf},
    process,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use alloy_primitives::hex;
use bip39::Mnemonic;
use clap::Parser;
use libp2p_identity::secp256k1;
use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;
use ream::{
    cli::{
        Cli, Commands,
        account_manager::AccountManagerConfig,
        beacon_node::BeaconNodeConfig,
        generate_private_key::GeneratePrivateKeyConfig,
        generate_validator_registry::run_generate_validator_registry,
        import_keystores::{load_keystore_directory, load_password_from_config, process_password},
        lean_node::LeanNodeConfig,
        validator_node::ValidatorNodeConfig,
        voluntary_exit::VoluntaryExitConfig,
    },
    startup_message::startup_message,
};
use ream_account_manager::{message_types::MessageType, seed::derive_seed_with_user_input};
use ream_api_types_beacon::id::ValidatorID;
use ream_api_types_common::{content_type::ContentType, id::ID};
use ream_chain_beacon::beacon_chain::BeaconChain;
use ream_chain_lean::{
    messages::LeanChainServiceMessage, p2p_request::LeanP2PRequest, service::LeanChainService,
};
use ream_checkpoint_sync_beacon::initialize_db_from_checkpoint;
use ream_checkpoint_sync_lean::{LeanCheckpointClient, verify_checkpoint_state};
#[cfg(feature = "devnet2")]
use ream_consensus_lean::attestation::AggregatedAttestations;
#[cfg(feature = "devnet1")]
use ream_consensus_lean::attestation::Attestation;
#[cfg(feature = "devnet2")]
use ream_consensus_lean::block::BlockSignatures;
use ream_consensus_lean::{
    attestation::AttestationData,
    block::{Block, BlockBody, BlockWithAttestation, SignedBlockWithAttestation},
    checkpoint::Checkpoint,
    validator::Validator,
};
use ream_consensus_misc::{
    constants::beacon::set_genesis_validator_root, misc::compute_epoch_at_slot,
};
use ream_events_beacon::BeaconEvent;
use ream_execution_engine::ExecutionEngine;
use ream_executor::ReamExecutor;
use ream_fork_choice_lean::{genesis::setup_genesis, store::Store};
use ream_keystore::keystore::EncryptedKeystore;
use ream_metrics::{NODE_INFO, NODE_START_TIME_SECONDS, set_int_gauge_vec};
use ream_network_manager::service::NetworkManagerService;
use ream_network_spec::networks::{
    beacon_network_spec, lean_network_spec, set_beacon_network_spec, set_lean_network_spec,
};
use ream_node::version::REAM_VERSION;
use ream_operation_pool::OperationPool;
use ream_p2p::{
    gossipsub::lean::{
        configurations::LeanGossipsubConfig,
        topics::{LeanGossipTopic, LeanGossipTopicKind},
    },
    network::lean::{LeanNetworkConfig, LeanNetworkService},
};
#[cfg(feature = "devnet2")]
use ream_post_quantum_crypto::leansig::signature::Signature;
use ream_post_quantum_crypto::leansig::{
    private_key::PrivateKey as LeanSigPrivateKey, public_key::PublicKey,
};
use ream_rpc_common::config::RpcServerConfig;
use ream_storage::{
    cache::{BeaconCacheDB, LeanCacheDB},
    db::{ReamDB, reset_db},
    dir::setup_data_dir,
    tables::table::REDBTable,
};
use ream_sync::rwlock::Writer;
use ream_sync_committee_pool::SyncCommitteePool;
use ream_validator_beacon::{
    beacon_api_client::BeaconApiClient,
    builder::builder_client::{BuilderClient, BuilderConfig},
    validator::ValidatorService,
    voluntary_exit::process_voluntary_exit,
};
use ream_validator_lean::{
    registry::load_validator_registry, service::ValidatorService as LeanValidatorService,
};
use ssz_types::VariableList;
use tokio::{
    sync::{broadcast, mpsc},
    time,
    time::Instant,
};
use tracing::{Instrument, error, info};
use tracing_subscriber::EnvFilter;
use tree_hash::TreeHash;

pub const APP_NAME: &str = "ream";

struct AbortOnDrop<T>(tokio::task::JoinHandle<T>);

impl<T> Drop for AbortOnDrop<T> {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// Entry point for the Ream client. Initializes logging, parses CLI arguments, and runs the
/// appropriate node type (beacon node, validator node, or account manager) based on the command
/// line arguments. Handles graceful shutdown on Ctrl-C.
fn main() {
    let cli = Cli::parse();

    // Set the default log level based on verbosity flag or RUST_LOG env var
    let rust_log = env::var(EnvFilter::DEFAULT_ENV).unwrap_or_default();
    let env_filter = match rust_log.is_empty() {
        true => EnvFilter::builder().parse_lossy(cli.verbosity.directive()),
        false => EnvFilter::builder().parse_lossy(rust_log),
    };
    tracing_subscriber::fmt().with_env_filter(env_filter).init();
    info!("\n{}", startup_message());

    let executor = ReamExecutor::new().expect("unable to create executor");
    let executor_clone = executor.clone();
    let ream_dir = setup_data_dir(APP_NAME, cli.data_dir.clone(), cli.ephemeral)
        .expect("Unable to initialize database directory");

    if cli.purge_db {
        reset_db(&ream_dir).expect("Unable to delete database");
    }

    let task_handle = match cli.command {
        Commands::LeanNode(config) => {
            let ream_db = ReamDB::new(ream_dir.clone()).expect("unable to init Ream Database");
            executor_clone.spawn(async move { run_lean_node(*config, executor, ream_db).await })
        }
        Commands::BeaconNode(config) => {
            let ream_db = ReamDB::new(ream_dir.clone()).expect("unable to init Ream Database");
            executor_clone.spawn(async move { run_beacon_node(*config, executor, ream_db).await })
        }
        Commands::ValidatorNode(config) => {
            executor_clone.spawn(async move { run_validator_node(*config, executor).await })
        }
        Commands::AccountManager(config) => {
            executor_clone.spawn(async move { run_account_manager(*config, ream_dir).await })
        }
        Commands::VoluntaryExit(config) => {
            executor_clone.spawn(async move { run_voluntary_exit(*config).await })
        }
        Commands::GeneratePrivateKey(config) => {
            executor_clone.spawn(async move { run_generate_private_key(*config).await })
        }
        Commands::GenerateKeystore(config) => {
            run_generate_validator_registry(*config).expect("failed to generate hash-sig keystore");
            process::exit(0);
        }
    };

    executor_clone.runtime().block_on(async {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                info!("Ctrl-C received, shutting down...");
            }
            _ = task_handle => {
                info!("Service exited, shutting down...");
            }
        }

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

        // Set node info metrics
        set_int_gauge_vec(&NODE_INFO, 1, &["ream", REAM_VERSION]);
        set_int_gauge_vec(
            &NODE_START_TIME_SECONDS,
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("Time went backwards")
                .as_secs() as i64,
            &[],
        );
    }

    let keystores = load_validator_registry(&config.validator_registry_path, &config.node_id)
        .expect("Failed to load validator registry");

    // Fill in which devnet we are running
    set_lean_network_spec(Arc::new(config.network));

    // Initialize the lean database
    let cache = Arc::new(LeanCacheDB::new());
    let lean_db = ream_db
        .init_lean_db()
        .expect("unable to init Ream Lean Database")
        .with_cache(cache);

    info!("ream lean database has been initialized");

    // Initialize the services that will run in the lean node.
    let (chain_sender, chain_receiver) = mpsc::unbounded_channel::<LeanChainServiceMessage>();
    let (outbound_p2p_sender, outbound_p2p_receiver) = mpsc::unbounded_channel::<LeanP2PRequest>();

    let (anchor_block, anchor_state) = if let Some(url) = config.checkpoint_sync_url.clone() {
        let state = LeanCheckpointClient::new()
            .fetch_finalized_state(&url)
            .await
            .expect("Failed to fetch checkpoint state");

        if !verify_checkpoint_state(&state) {
            panic!("Downloaded checkpoint state failed to verify");
        }

        let block = Block {
            slot: state.slot,
            proposer_index: state.latest_block_header.proposer_index,
            parent_root: state.latest_block_header.parent_root,
            state_root: state.tree_hash_root(),
            body: BlockBody {
                attestations: Default::default(),
            },
        };

        (block, state)
    } else {
        let validators = lean_network_spec()
            .validator_public_keys
            .iter()
            .enumerate()
            .map(|(index, public_key)| Validator {
                public_key: PublicKey::new(*public_key),
                index: index as u64,
            })
            .collect::<Vec<_>>();

        setup_genesis(lean_network_spec().genesis_time, validators)
    };

    let attestation_data = AttestationData {
        slot: anchor_state.slot,
        head: Checkpoint {
            root: anchor_state.latest_block_header.tree_hash_root(),
            slot: anchor_state.slot,
        },
        target: anchor_state.latest_finalized,
        source: anchor_state.latest_justified,
    };

    let (lean_chain_writer, lean_chain_reader) = Writer::new(
        Store::get_forkchoice_store(
            SignedBlockWithAttestation {
                message: BlockWithAttestation {
                    block: anchor_block,
                    #[cfg(feature = "devnet1")]
                    proposer_attestation: Attestation {
                        validator_id: 0,
                        data: attestation_data,
                    },
                    #[cfg(feature = "devnet2")]
                    proposer_attestation: AggregatedAttestations {
                        validator_id: 0,
                        data: attestation_data,
                    },
                },
                #[cfg(feature = "devnet1")]
                signature: VariableList::default(),
                #[cfg(feature = "devnet2")]
                signature: BlockSignatures {
                    attestation_signatures: VariableList::default(),
                    proposer_signature: Signature::blank(),
                },
            },
            anchor_state,
            lean_db,
            None,
        )
        .expect("Could not get forkchoice store"),
    );

    let network_state = lean_chain_reader.read().await.network_state.clone();

    // Initialize the lean network service

    let fork = "devnet0".to_string();
    let topics: Vec<LeanGossipTopic> = vec![
        LeanGossipTopic {
            fork: fork.clone(),
            kind: LeanGossipTopicKind::Block,
        },
        LeanGossipTopic {
            fork,
            kind: LeanGossipTopicKind::Attestation,
        },
    ];

    let mut network_service = LeanNetworkService::new(
        Arc::new(LeanNetworkConfig {
            gossipsub_config: LeanGossipsubConfig {
                topics,
                ..Default::default()
            },
            socket_address: config.socket_address,
            socket_port: config.socket_port,
            private_key_path: config.private_key_path,
        }),
        executor.clone(),
        chain_sender.clone(),
        outbound_p2p_receiver,
        network_state.clone(),
    )
    .await
    .expect("Failed to create network service");

    let chain_service =
        LeanChainService::new(lean_chain_writer, chain_receiver, outbound_p2p_sender).await;

    let validator_service = LeanValidatorService::new(keystores, chain_sender).await;

    let server_config = RpcServerConfig::new(
        config.http_address,
        config.http_port,
        config.http_allow_origin,
    );

    // Start the services concurrently.
    let mut chain_task =
        AbortOnDrop(executor.spawn(async move { chain_service.start().await }.in_current_span()));
    let mut network_task = AbortOnDrop(
        executor
            .spawn(async move { network_service.start(config.bootnodes).await }.in_current_span()),
    );
    let mut validator_task = AbortOnDrop(
        executor.spawn(async move { validator_service.start().await }.in_current_span()),
    );
    let mut http_task = AbortOnDrop(
        executor.spawn(
            async move {
                ream_rpc_lean::server::start(server_config, lean_chain_reader, network_state).await
            }
            .in_current_span(),
        ),
    );

    executor.spawn(async move {
        countdown_for_genesis().await;
    });

    tokio::select! {
        result = &mut chain_task.0 => {
            error!("Chain service has stopped unexpectedly: {result:?}");
        },
        result = &mut network_task.0 => {
            error!("Network service has stopped unexpectedly: {result:?}");
        },
        result = &mut validator_task.0 => {
            error!("Validator service has stopped unexpectedly: {result:?}");
        },
        result = &mut http_task.0 => {
            error!("RPC service has stopped unexpectedly: {result:?}");
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
    let cache = Arc::new(BeaconCacheDB::new());
    let beacon_db = ream_db
        .init_beacon_db()
        .expect("unable to init Ream Beacon Database")
        .with_cache(cache.clone());

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
            .state_provider()
            .get(oldest_root)
            .expect("Failed to access beacon state provider")
            .expect("No beacon state found")
            .genesis_validators_root,
    );

    let operation_pool = Arc::new(OperationPool::default());
    let sync_committee_pool = Arc::new(SyncCommitteePool::default());

    let (event_sender, _) = broadcast::channel::<BeaconEvent>(1024);

    let server_config = RpcServerConfig::new(
        config.http_address,
        config.http_port,
        config.http_allow_origin,
    );

    // Initialize builder client if enabled
    let builder_client = config.enable_builder.then(|| {
        let mev_relay_url = config
            .mev_relay_url
            .clone()
            .expect("MEV relay URL must be present when builder is enabled");
        let builder_config = BuilderConfig {
            builder_enabled: true,
            mev_relay_url,
        };
        Arc::new(
            BuilderClient::new(builder_config, Duration::from_secs(30), ContentType::Json)
                .expect("Failed to create builder client"),
        )
    });

    // Create execution engine if configured
    let execution_engine = if let (Some(execution_endpoint), Some(jwt_path)) = (
        config.execution_endpoint.clone(),
        config.execution_jwt_secret.clone(),
    ) {
        Some(
            ExecutionEngine::new(execution_endpoint, jwt_path)
                .expect("Failed to create execution engine"),
        )
    } else {
        None
    };

    // Create beacon chain
    let beacon_chain = Arc::new(BeaconChain::new(
        beacon_db.clone(),
        operation_pool.clone(),
        sync_committee_pool.clone(),
        execution_engine.clone(),
        Some(event_sender.clone()),
    ));

    // Create network manager
    let network_manager = NetworkManagerService::new(
        executor.clone(),
        config.into(),
        beacon_db.clone(),
        beacon_db.data_dir.clone(),
        beacon_chain.clone(),
        sync_committee_pool.clone(),
        cache.clone(),
    )
    .await
    .expect("Failed to create manager service");

    let network_state = network_manager.network_state.clone();
    let p2p_sender = Arc::new(network_manager.p2p_sender.clone());

    let mut network_task = AbortOnDrop(executor.spawn(async move {
        network_manager.start().await;
    }));

    let mut http_task = AbortOnDrop(executor.spawn(async move {
        ream_rpc_beacon::server::start(
            server_config,
            beacon_db,
            network_state,
            operation_pool,
            sync_committee_pool,
            execution_engine,
            builder_client,
            event_sender,
            beacon_chain,
            p2p_sender,
            cache,
        )
        .await
    }));

    tokio::select! {
        _ = &mut http_task.0 => {
            info!("HTTP server stopped!");
        },
        _ = &mut network_task.0 => {
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

        let (public_key, _private_key) = LeanSigPrivateKey::generate_key_pair(
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

// Countdown logs until the genesis timestamp reaches
pub async fn countdown_for_genesis() {
    loop {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::MAX)
            .as_secs();
        let genesis = lean_network_spec().genesis_time;

        if now >= genesis {
            // Only log the "Genesis reached" message if we are starting within
            // a small 2-second window of the actual event.
            if now <= genesis + 2 {
                info!("Genesis reached! Starting services...");
            }
            break;
        }

        let remaining = lean_network_spec().genesis_time.saturating_sub(now);

        // Format the remaining time for a cleaner log
        let minutes = (remaining % 3600) / 60;
        let seconds = remaining % 60;

        info!(
            "Waiting for genesis in {:02}:{:02} seconds",
            minutes, seconds
        );

        // Sleep for 1 second before ticking again
        time::sleep(Duration::from_secs(1)).await;
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;
    #[cfg(feature = "devnet1")]
    use std::{fs, path::PathBuf};

    #[cfg(feature = "devnet1")]
    use alloy_primitives::hex;
    use clap::Parser;
    #[cfg(feature = "devnet1")]
    use libp2p_identity::{Keypair, secp256k1};
    #[cfg(feature = "devnet1")]
    use ream::cli::verbosity::Verbosity;
    use ream::cli::{Cli, Commands};
    use ream_executor::ReamExecutor;
    #[cfg(feature = "devnet1")]
    use ream_storage::tables::{field::REDBField, table::REDBTable};
    use ream_storage::{db::ReamDB, dir::setup_data_dir};
    use tokio::time::{sleep, timeout};
    #[cfg(feature = "devnet1")]
    use tracing::info;

    use crate::{APP_NAME, run_lean_node};

    #[test]
    fn test_lean_node_runs_10_seconds_without_panicking() {
        let cli = Cli::parse_from([
            "ream",
            "--ephemeral",
            "lean_node",
            "--network",
            "ephemery",
            "--validator-registry-path",
            "./assets/lean/validators.yaml",
        ]);

        let Commands::LeanNode(config) = cli.command else {
            panic!("Expected lean_node command");
        };

        let ream_dir = setup_data_dir(APP_NAME, None, true).unwrap();
        let db = ReamDB::new(ream_dir).unwrap();
        let executor = ReamExecutor::new().unwrap();
        let executor_handle = executor.clone();

        executor.runtime().block_on(async move {
            let handle = tokio::spawn(async move {
                run_lean_node(*config, executor_handle, db).await;
            });

            let result = timeout(Duration::from_secs(10), async {
                sleep(Duration::from_secs(10)).await;
                Ok::<_, ()>(())
            })
            .await;

            match result {
                Ok(Ok(())) => {}
                Err(err) => panic!("lean_node panicked or exited early {err:?}"),
                Ok(Err(err)) => panic!("internal error {err:?}"),
            }

            handle.abort();
        });
    }

    /// TODO: Get finalization working for devnet2
    #[test]
    #[cfg(feature = "devnet1")]
    fn test_lean_node_finalizes() {
        let cli = Cli::parse_from([
            "ream",
            "--ephemeral",
            "lean_node",
            "--network",
            "ephemery",
            "--validator-registry-path",
            "./assets/lean/validators.yaml",
            "--socket-port",
            "9001",
            "--http-port",
            "5053",
        ]);

        let Commands::LeanNode(config) = cli.command else {
            panic!("Expected lean_node command");
        };

        let ream_dir = setup_data_dir(APP_NAME, None, true).unwrap();
        let db = ReamDB::new(ream_dir).unwrap();
        let executor = ReamExecutor::new().unwrap();
        let executor_handle = executor.clone();

        let cloned_db = db.clone();
        executor.runtime().block_on(async move {
            let handle = tokio::spawn(async move {
                run_lean_node(*config, executor_handle, cloned_db).await;
            });

            let result = timeout(Duration::from_secs(60), async {
                sleep(Duration::from_secs(60)).await;
                Ok::<_, ()>(())
            })
            .await;

            match result {
                Ok(Ok(())) => {}
                Err(err) => panic!("lean_node panicked or exited early {err:?}"),
                Ok(Err(err)) => panic!("internal error {err:?}"),
            }

            handle.abort();
        });

        let lean_db = db.init_lean_db().unwrap();
        let head = lean_db.head_provider().get().unwrap();
        let head_state = lean_db.state_provider().get(head).unwrap().unwrap();

        let justfication_lag = 4;
        let finalization_lag = 5;

        assert!(
            head_state.slot > finalization_lag,
            "Expected the head slot to be greater than finalization lag"
        );
        assert!(
            head_state.latest_finalized.slot > 0,
            "Expected the finalized checkpoint to have advanced from genesis current slot {} finalized slot {}",
            head_state.slot,
            head_state.latest_finalized.slot
        );
        assert!(
            head_state.latest_justified.slot + justfication_lag == head_state.slot,
            "Expected the head to be at least {justfication_lag} slots ahead of the justified checkpoint {:?} + {justfication_lag} vs {:?}",
            head_state.latest_justified.slot,
            head_state.slot
        );
        assert!(
            head_state.latest_finalized.slot + finalization_lag == head_state.slot,
            "Expected the head to be at least {finalization_lag} slots ahead of the finalized checkpoint {:?} + {finalization_lag} vs {:?}",
            head_state.latest_finalized.slot,
            head_state.slot
        );
    }

    #[cfg(feature = "devnet1")]
    fn generate_node_identity(path: &PathBuf) -> String {
        let secp256k1_key = secp256k1::Keypair::generate();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("Failed to create key dir");
        }
        fs::write(path, hex::encode(secp256k1_key.secret().to_bytes()))
            .expect("Failed to write private key");
        let id_keypair = Keypair::from(secp256k1_key);
        id_keypair.public().to_peer_id().to_string()
    }

    #[test]
    #[cfg(feature = "devnet1")]
    #[ignore = "I am not sure if this topology is supposed to work or not"]
    fn test_lean_node_finalizes_linear_1_2_1() {
        let topology = vec![vec![], vec![0], vec![1]];
        run_multi_node_finalization_test(topology, "linear_1_2_1");
    }

    #[test]
    #[cfg(feature = "devnet1")]
    fn test_lean_node_finalizes_mesh_2_2_2() {
        let topology = vec![vec![], vec![0], vec![0, 1]];
        run_multi_node_finalization_test(topology, "mesh_2_2_2");
    }

    #[cfg(feature = "devnet1")]
    fn run_multi_node_finalization_test(topology: Vec<Vec<usize>>, test_name: &str) {
        if true {
            let _ = tracing_subscriber::fmt()
                .with_env_filter(Verbosity::Info.directive())
                .with_test_writer()
                .try_init();
        }

        info!("Starting multi-node finalization test: {}", test_name);

        let test_duration_secs = 60;
        let base_p2p_port = 20600;
        let base_http_port = 16652;
        let node_count = topology.len();

        let potential_paths = vec![
            PathBuf::from("bin/ream/assets/lean"),
            PathBuf::from("assets/lean"),
            PathBuf::from("../assets/lean"),
        ];

        let assets_directory = potential_paths
            .into_iter()
            .find(|p| p.exists())
            .expect("Could not find 'assets/lean' directory.")
            .canonicalize()
            .expect("Failed to canonicalize assets path");

        let registry_path =
            assets_directory.join(format!("test_multi_node_registry_{test_name}.yaml"));

        let mut validators_yaml = String::new();
        for i in 0..node_count {
            validators_yaml.push_str(&format!("node{}:\n  - {i}\n", i + 1));
        }

        fs::write(&registry_path, validators_yaml).expect("Failed to write temp registry");
        let registry_path_str = registry_path.to_string_lossy().to_string();

        let executor = ReamExecutor::new().unwrap();
        executor.clone().runtime().block_on(async move {
            let mut node_handles = Vec::new();
            let mut node_addresses: Vec<String> = Vec::new();
            let mut db_instances = Vec::new();

            for (i, node_boot_config) in topology.iter().enumerate() {
                let node_index = i + 1;
                let node_id = format!("node{node_index}");

                let ream_dir =
                    std::env::temp_dir().join(format!("{APP_NAME}_{test_name}_node_{node_index}"));

                if ream_dir.exists() {
                    let _ = fs::remove_dir_all(&ream_dir);
                }
                fs::create_dir_all(&ream_dir).expect("Failed to create data dir");

                let key_path = ream_dir.join("node_key");
                let peer_id = generate_node_identity(&key_path);

                let port_offset = (test_name.len() as u16) % 100;
                let p2p_port = base_p2p_port + port_offset + (i as u16);
                let http_port = base_http_port + port_offset + (i as u16);

                let address = format!("/ip4/127.0.0.1/udp/{p2p_port}/quic-v1/p2p/{peer_id}");
                node_addresses.push(address.clone());

                if i == 0 {
                    info!("BOOTNODE ADDRESS: {address}");
                }

                let db = ReamDB::new(ream_dir.clone()).unwrap();
                db_instances.push(db.clone());

                let mut bootnode_arguments: Vec<String> = Vec::new();
                for &target_idx in node_boot_config {
                    if target_idx < node_addresses.len() {
                        bootnode_arguments.push(node_addresses[target_idx].clone());
                    }
                }

                let mut arguments = vec![
                    "ream".to_string(),
                    "lean_node".to_string(),
                    "--network".to_string(),
                    "ephemery".to_string(),
                    "--validator-registry-path".to_string(),
                    registry_path_str.clone(),
                    "--socket-port".to_string(),
                    p2p_port.to_string(),
                    "--socket-address".to_string(),
                    "127.0.0.1".to_string(),
                    "--http-port".to_string(),
                    http_port.to_string(),
                    "--node-id".to_string(),
                    node_id.clone(),
                    "--private-key-path".to_string(),
                    key_path.to_string_lossy().to_string(),
                ];

                if !bootnode_arguments.is_empty() {
                    arguments.push("--bootnodes".to_string());
                    arguments.push(bootnode_arguments.join(","));
                }

                let cli = Cli::parse_from(arguments);
                let Commands::LeanNode(config) = cli.command else {
                    panic!("Expected lean_node command");
                };

                let node_executor = executor.clone();
                let handle = tokio::spawn(async move {
                    run_lean_node(*config, node_executor, db).await;
                });
                node_handles.push(handle);

                if i == 0 {
                    info!("Waiting 5s for Bootnode to initialize QUIC listener...");
                    sleep(Duration::from_secs(5)).await;
                }
            }

            info!(
                "All nodes started. Monitoring for {} seconds...",
                test_duration_secs
            );

            let db_instances_monitor = db_instances.clone();
            let monitor_handle = tokio::spawn(async move {
                let start = std::time::Instant::now();
                loop {
                    if start.elapsed().as_secs() >= test_duration_secs {
                        break;
                    }
                    sleep(Duration::from_secs(2)).await;

                    let db = &db_instances_monitor[0];
                    if let Ok(lean_db) = db.init_lean_db()
                        && let Ok(head) = lean_db.head_provider().get()
                        && let Ok(Some(state)) = lean_db.state_provider().get(head)
                    {
                        info!(
                            "Node 1 Chain: Slot={} | Finalized={}",
                            state.slot, state.latest_finalized.slot
                        );
                    }
                }
            });

            let _ = timeout(Duration::from_secs(test_duration_secs + 5), monitor_handle).await;

            let _ = fs::remove_file(&registry_path);
            for handle in node_handles {
                handle.abort();
            }

            let lean_db = db_instances[0].init_lean_db().unwrap();
            let head = lean_db.head_provider().get().expect("Failed to get head");
            let head_state = lean_db
                .state_provider()
                .get(head)
                .unwrap()
                .expect("Failed to get head state");

            info!(
                "FINAL: Node 1 Slot: {}, Finalized: {}",
                head_state.slot, head_state.latest_finalized.slot
            );

            let finalization_lag = 5;

            assert!(
                head_state.slot > finalization_lag,
                "Chain did not advance enough. Current: {}, Request: {finalization_lag}",
                head_state.slot,
            );
            assert!(
                head_state.latest_finalized.slot > 0,
                "NO FINALIZATION. Check P2P logs for 'Dial error' or 'Handshake failed'."
            );
        });
    }

    #[test]
    #[cfg(feature = "devnet1")]
    fn test_lean_node_syncs_and_finalizes_late_joiner() {
        // Topology: Node 3 connects to Node 1 and Node 2.
        // Node 1 and 2 start immediately. Node 3 starts after 50s.
        let topology = [vec![], vec![0], vec![0, 1]];
        let test_name = "late_joiner_sync";

        if true {
            let _ = tracing_subscriber::fmt()
                .with_env_filter(Verbosity::Info.directive())
                .with_test_writer()
                .try_init();
        }

        info!(
            "Starting multi-node finalization test with late joiner: {}",
            test_name
        );

        let test_duration_secs = 120;
        let late_start_delay = 50;
        let base_p2p_port = 21600;
        let base_http_port = 17652;
        let node_count = topology.len();

        let potential_paths = vec![
            PathBuf::from("bin/ream/assets/lean"),
            PathBuf::from("assets/lean"),
            PathBuf::from("../assets/lean"),
        ];

        let assets_directory = potential_paths
            .into_iter()
            .find(|p| p.exists())
            .expect("Could not find 'assets/lean' directory.")
            .canonicalize()
            .expect("Failed to canonicalize assets path");

        let registry_path =
            assets_directory.join(format!("test_multi_node_registry_{test_name}.yaml"));

        let mut validators_yaml = String::new();
        for i in 0..node_count {
            validators_yaml.push_str(&format!("node{}:\n  - {i}\n", i + 1));
        }

        fs::write(&registry_path, validators_yaml).expect("Failed to write temp registry");
        let registry_path_str = registry_path.to_string_lossy().to_string();

        let executor = ReamExecutor::new().unwrap();
        executor.clone().runtime().block_on(async move {
            let mut node_handles = Vec::with_capacity(node_count);
            for _ in 0..node_count {
                node_handles.push(None);
            }

            let mut node_addresses: Vec<String> = Vec::new();
            let mut db_instances = Vec::with_capacity(node_count);
            for _ in 0..node_count {
                db_instances.push(None);
            }

            let mut prepared_nodes = Vec::new();

            for (i, _) in topology.iter().enumerate() {
                let node_index = i + 1;

                let ream_dir =
                    std::env::temp_dir().join(format!("{APP_NAME}_{test_name}_node_{node_index}"));

                if ream_dir.exists() {
                    let _ = fs::remove_dir_all(&ream_dir);
                }
                fs::create_dir_all(&ream_dir).expect("Failed to create data dir");

                let key_path = ream_dir.join("node_key");
                let peer_id = generate_node_identity(&key_path);

                let port_offset = (test_name.len() as u16) % 100;
                let p2p_port = base_p2p_port + port_offset + (i as u16);

                let address = format!("/ip4/127.0.0.1/udp/{p2p_port}/quic-v1/p2p/{peer_id}");
                node_addresses.push(address.clone());

                let db = ReamDB::new(ream_dir.clone()).unwrap();
                db_instances[i] = Some(db.clone());
            }

            for (i, node_boot_config) in topology.iter().enumerate() {
                let node_index = i + 1;
                let db = db_instances[i].clone().unwrap();
                let key_path = std::env::temp_dir()
                    .join(format!("{APP_NAME}_{test_name}_node_{node_index}"))
                    .join("node_key");

                let port_offset = (test_name.len() as u16) % 100;
                let p2p_port = base_p2p_port + port_offset + (i as u16);
                let http_port = base_http_port + port_offset + (i as u16);

                let mut bootnode_arguments: Vec<String> = Vec::new();
                for &target_idx in node_boot_config {
                    if target_idx < node_addresses.len() {
                        bootnode_arguments.push(node_addresses[target_idx].clone());
                    }
                }

                let mut arguments = vec![
                    "ream".to_string(),
                    "lean_node".to_string(),
                    "--network".to_string(),
                    "ephemery".to_string(),
                    "--validator-registry-path".to_string(),
                    registry_path_str.clone(),
                    "--socket-port".to_string(),
                    p2p_port.to_string(),
                    "--socket-address".to_string(),
                    "127.0.0.1".to_string(),
                    "--http-port".to_string(),
                    http_port.to_string(),
                    "--node-id".to_string(),
                    format!("node{node_index}"),
                    "--private-key-path".to_string(),
                    key_path.to_string_lossy().to_string(),
                ];

                if !bootnode_arguments.is_empty() {
                    arguments.push("--bootnodes".to_string());
                    arguments.push(bootnode_arguments.join(","));
                }

                let cli = Cli::parse_from(arguments);
                let Commands::LeanNode(config) = cli.command else {
                    panic!("Expected lean_node command");
                };

                prepared_nodes.push((*config, db));
            }

            info!("Starting initial nodes (1 and 2)...");
            for i in 0..2 {
                use tracing::{Instrument, info_span};

                let node = &prepared_nodes[i];
                let config = node.0.clone();
                let db = node.1.clone();
                let node_executor = executor.clone();

                let span = info_span!(
                    "lean_node",
                    node_id = %config.node_id
                );

                let handle = tokio::spawn(
                    async move {
                        run_lean_node(config, node_executor, db).await;
                    }
                    .instrument(span),
                );

                node_handles[i] = Some(handle);
            }

            sleep(Duration::from_secs(5)).await;

            let start_time = std::time::Instant::now();
            let mut node_3_started = false;

            loop {
                let elapsed = start_time.elapsed().as_secs();
                if elapsed >= test_duration_secs {
                    break;
                }

                if !node_3_started && elapsed >= late_start_delay {
                    use tracing::{Instrument, info_span};

                    info!("Starting Late Joiner Node 3...");

                    let node = &prepared_nodes[2];
                    let config = node.0.clone();
                    let db = node.1.clone();
                    let node_executor = executor.clone();

                    let span = info_span!(
                        "lean_node",
                        node_id = %config.node_id
                    );

                    let handle = tokio::spawn(
                        async move {
                            run_lean_node(config, node_executor, db).await;
                        }
                        .instrument(span),
                    );

                    node_handles[2] = Some(handle);
                    node_3_started = true;
                }

                sleep(Duration::from_secs(2)).await;

                for (i, db_instance) in db_instances.iter().enumerate().take(node_count) {
                    if let Some(db) = db_instance
                        && let Ok(lean_db) = db.init_lean_db()
                        && let Ok(head) = lean_db.head_provider().get()
                        && let Ok(Some(state)) = lean_db.state_provider().get(head)
                    {
                        info!(
                            "Node {} Chain: Slot={} | Finalized={}",
                            i + 1,
                            state.slot,
                            state.latest_finalized.slot
                        );
                    }
                }
            }

            let _ = fs::remove_file(&registry_path);
            for handle in node_handles.into_iter().flatten() {
                handle.abort();
            }

            let lean_db = db_instances[2].as_ref().unwrap().init_lean_db().unwrap();
            let head = lean_db.head_provider().get().expect("Failed to get head");
            let head_state = lean_db
                .state_provider()
                .get(head)
                .unwrap()
                .expect("Failed to get head state");

            info!(
                "FINAL: Node 3 Slot: {}, Finalized: {}",
                head_state.slot, head_state.latest_finalized.slot
            );

            assert!(
                head_state.latest_finalized.slot > 0,
                "Node 3 failed to finalize. Finalized slot: {}",
                head_state.latest_finalized.slot
            );

            let lean_db_1 = db_instances[0].as_ref().unwrap().init_lean_db().unwrap();
            let head_state_1 = lean_db_1
                .state_provider()
                .get(lean_db_1.head_provider().get().unwrap())
                .unwrap()
                .unwrap();
            info!(
                "FINAL: Node 1 Slot: {}, Finalized: {}",
                head_state_1.slot, head_state_1.latest_finalized.slot
            );

            assert!(
                head_state.slot == head_state_1.slot,
                "Node 3 is too far behind Node 1. Node 3: {}, Node 1: {}",
                head_state.slot,
                head_state_1.slot
            );
        });
    }
}
