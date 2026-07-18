use std::{
    collections::BTreeSet,
    env, fs,
    net::SocketAddr,
    path::{Path, PathBuf},
    process,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use alloy_primitives::hex;
use bip39::Mnemonic;
use libp2p_identity::secp256k1;
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
        verbosity::Verbosity,
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
use ream_checkpoint_sync_beacon::{
    initialize_db_from_checkpoint, initialize_db_from_genesis_state,
};
use ream_checkpoint_sync_lean::{LeanCheckpointClient, verify_checkpoint_state};
#[cfg(feature = "devnet5")]
use ream_consensus_lean::attestation::MultiMessageAggregate;
use ream_consensus_lean::{block::SignedBlock, validator::Validator};
use ream_consensus_misc::{
    constants::{
        beacon::set_genesis_validator_root,
        lean::{attestation_committee_count, set_attestation_committee_count},
    },
    misc::compute_epoch_at_slot,
};
use ream_events_beacon::BeaconEvent;
use ream_execution_engine::ExecutionEngine;
use ream_executor::ReamExecutor;
use ream_fork_choice_lean::{
    genesis::setup_genesis,
    store::{Store, compute_subnet_id},
};
use ream_keystore::keystore::EncryptedKeystore;
use ream_metrics::{
    ATTESTATION_COMMITTEE_SUBNET, NODE_INFO, NODE_START_TIME_SECONDS, init_node_metrics,
    set_int_gauge_vec,
};
use ream_network_manager::service::NetworkManagerService;
use ream_network_spec::networks::{
    beacon_network_spec, lean_network_spec, set_beacon_network_spec, set_lean_network_spec,
};
use ream_network_state_lean::AggregatorState;
use ream_node::version::REAM_VERSION;
use ream_operation_pool::OperationPool;
use ream_p2p::{
    gossipsub::lean::{
        configurations::LeanGossipsubConfig,
        topics::{LeanGossipTopic, LeanGossipTopicKind},
    },
    network::lean::{LeanNetworkConfig, LeanNetworkService},
};
#[cfg(feature = "devnet5")]
use ream_post_quantum_crypto::lean_multisig::type_2::{type_2_setup, type_2_setup_verifier};
use ream_post_quantum_crypto::leansig::{
    private_key::PrivateKey as LeanSigPrivateKey, public_key::PublicKey,
};
use ream_rpc_common::config::RpcServerConfig;
use ream_rpc_lean::{handlers::test_driver::test_driver_enabled, server::start_test_driver};
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
    sync::{broadcast, mpsc::unbounded_channel},
    time::{self, Instant},
};
use tracing::{Instrument, error, info};
use tracing_subscriber::EnvFilter;

#[global_allocator]
static ALLOC: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

pub const APP_NAME: &str = "ream";
const DEFAULT_QUIET_LOG_TARGETS: &str = "libp2p_gossipsub::behaviour=error";

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
    let cli = Cli::parse_validated();

    // Set the default log level based on verbosity flag or RUST_LOG env var
    let rust_log = env::var(EnvFilter::DEFAULT_ENV).unwrap_or_default();
    let env_filter = match rust_log.is_empty() {
        true => {
            let env_filter = EnvFilter::builder().parse_lossy(cli.verbosity.directive());

            match cli.verbosity {
                Verbosity::Debug | Verbosity::Trace => env_filter,
                _ => env_filter.add_directive(
                    DEFAULT_QUIET_LOG_TARGETS
                        .parse()
                        .expect("valid gossipsub tracing directive"),
                ),
            }
        }
        false => EnvFilter::builder().parse_lossy(rust_log),
    };
    tracing_subscriber::fmt().with_env_filter(env_filter).init();
    info!("\n{}", startup_message());

    let executor = ReamExecutor::new().expect("unable to create executor");
    let executor_clone = executor.clone();
    let ream_directory = setup_data_dir(APP_NAME, cli.data_dir.clone(), cli.ephemeral)
        .expect("Unable to initialize database directory");

    if cli.purge_db {
        reset_db(&ream_directory).expect("Unable to delete database");
    }

    let task_handle = match cli.command {
        Commands::LeanNode(config) => {
            let ream_db =
                ReamDB::new(ream_directory.clone()).expect("unable to init Ream Database");
            executor_clone.spawn(async move { run_lean_node(*config, executor, ream_db).await })
        }
        Commands::BeaconNode(config) => {
            let ream_db =
                ReamDB::new(ream_directory.clone()).expect("unable to init Ream Database");
            executor_clone.spawn(async move { run_beacon_node(*config, executor, ream_db).await })
        }
        Commands::ValidatorNode(config) => {
            executor_clone.spawn(async move { run_validator_node(*config, executor).await })
        }
        Commands::AccountManager(config) => {
            executor_clone.spawn(async move { run_account_manager(*config, ream_directory).await })
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

    set_attestation_committee_count(config.attestation_committee_count);

    let keystores = load_validator_registry(&config.validator_registry_path, &config.node_id)
        .expect("Failed to load validator registry");

    if let Some(keystore) = keystores.first() {
        set_int_gauge_vec(
            &ATTESTATION_COMMITTEE_SUBNET,
            compute_subnet_id(keystore.index, attestation_committee_count()) as i64,
            &[],
        );
    }

    // Initialize aggregation verifier bytecode — all nodes need this to verify
    // aggregate signatures when processing blocks during sync.
    info!("Initializing aggregation verifier bytecode...");
    #[cfg(feature = "devnet5")]
    type_2_setup_verifier();
    info!("Aggregation verifier bytecode initialized");

    // Initialize aggregation prover bytecode only if this node is an aggregator.
    if config.is_aggregator {
        info!("Initializing aggregation prover bytecode for aggregator mode...");
        #[cfg(feature = "devnet5")]
        type_2_setup();
        info!("Aggregation prover bytecode initialized");
    }

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
    let (chain_sender, chain_receiver) = unbounded_channel::<LeanChainServiceMessage>();
    let (outbound_p2p_sender, outbound_p2p_receiver) = unbounded_channel::<LeanP2PRequest>();

    let (anchor_signed_block, anchor_state) = if let Some(url) = config.checkpoint_sync_url.clone()
    {
        let (state, signed_block) = LeanCheckpointClient::new()
            .fetch_finalized_anchor(&url)
            .await
            .expect("Failed to fetch checkpoint anchor");

        verify_checkpoint_state(&state).expect("Downloaded checkpoint state failed to verify");

        (signed_block, state)
    } else {
        let validators = lean_network_spec()
            .genesis_validators
            .iter()
            .enumerate()
            .map(|(index, entry)| Validator {
                attestation_public_key: PublicKey::new(entry.attestation_public_key),
                proposal_public_key: PublicKey::new(entry.proposal_public_key),
                index: index as u64,
            })
            .collect::<Vec<_>>();

        let (genesis_block, genesis_state) =
            setup_genesis(lean_network_spec().genesis_time, validators);
        let signed_genesis = SignedBlock {
            block: genesis_block,
            #[cfg(feature = "devnet5")]
            proof: MultiMessageAggregate {
                proof: VariableList::default(),
            },
        };
        (signed_genesis, genesis_state)
    };
    let (lean_chain_writer, lean_chain_reader) = Writer::new(
        Store::get_forkchoice_store(
            anchor_signed_block,
            anchor_state,
            lean_db,
            None,
            keystores.first().map(|keystore| keystore.index),
        )
        .expect("Could not get forkchoice store")
        .with_block_production_strategy(config.block_production),
    );

    let test_driver_enabled = test_driver_enabled();
    let network_state = lean_chain_reader.read().await.network_state.clone();

    let aggregator_state = Arc::new(AggregatorState::new(config.is_aggregator));

    if test_driver_enabled {
        let server_config = RpcServerConfig::new(
            config.http_address,
            config.http_port,
            config.http_allow_origin,
        );
        start_test_driver(
            server_config,
            lean_chain_reader,
            lean_chain_writer,
            network_state,
            aggregator_state,
        )
        .await
        .expect("Lean test-driver RPC service stopped unexpectedly");
        return;
    }

    // Initialize the lean network service
    let fork = "12345678".to_string();

    let committee_count = attestation_committee_count();
    let subscribed_subnets = {
        let mut set: BTreeSet<u64> = keystores
            .iter()
            .map(|keystore| keystore.index % committee_count)
            .collect();

        if config.is_aggregator {
            for subnet_id in &config.aggregate_subnet_ids {
                set.insert(*subnet_id);
            }

            if set.is_empty() {
                set.insert(0);
            }
        }

        set
    };

    let topics: Vec<LeanGossipTopic> = {
        let mut topics = vec![
            LeanGossipTopic {
                fork: fork.clone(),
                kind: LeanGossipTopicKind::Block,
            },
            LeanGossipTopic {
                fork: fork.clone(),
                kind: LeanGossipTopicKind::AggregatedAttestation,
            },
        ];
        for subnet_id in &subscribed_subnets {
            topics.push(LeanGossipTopic {
                fork: fork.clone(),
                kind: LeanGossipTopicKind::AttestationSubnet(*subnet_id),
            });
        }
        topics
    };

    info!(
        is_aggregator = config.is_aggregator,
        subscribed_subnets = ?subscribed_subnets,
        attestation_committee_count = committee_count,
        "Computed attestation subnet subscriptions"
    );

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

    let chain_service = LeanChainService::new(
        lean_chain_writer,
        chain_receiver,
        outbound_p2p_sender,
        aggregator_state.clone(),
    )
    .await;

    let arc_keystores: Vec<Arc<_>> = keystores.into_iter().map(Arc::new).collect();

    let validator_service = LeanValidatorService::new(arc_keystores, chain_sender).await;

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
                ream_rpc_lean::server::start(
                    server_config,
                    lean_chain_reader,
                    network_state,
                    aggregator_state,
                )
                .await
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
    run_beacon_node_inner(config, executor, ream_db, true).await;
}

// `initialize_globals` is `false` only in tests, which set the globals themselves beforehand.
async fn run_beacon_node_inner(
    config: BeaconNodeConfig,
    executor: ReamExecutor,
    ream_db: ReamDB,
    initialize_globals: bool,
) {
    info!("starting up beacon node...");

    if config.enable_metrics {
        let address = SocketAddr::new(config.metrics_address, config.metrics_port);
        prometheus_exporter::start(address).expect("Failed to start prometheus exporter");
        info!(
            "Metrics started on {}:{}",
            config.metrics_address, config.metrics_port
        );

        init_node_metrics();
    }

    if initialize_globals {
        set_beacon_network_spec(config.network.clone());
    }

    // Initialize the beacon database
    let cache = Arc::new(BeaconCacheDB::new());
    let beacon_db = ream_db
        .init_beacon_db()
        .expect("unable to init Ream Beacon Database")
        .with_cache(cache.clone());

    info!("ream beacon database has been initialized");

    if initialize_globals {
        if let Some(genesis_state_path) = &config.genesis_state_path {
            initialize_db_from_genesis_state(beacon_db.clone(), genesis_state_path)
                .expect("Unable to initialize database from genesis state");
        } else {
            let _is_ws_verified = initialize_db_from_checkpoint(
                beacon_db.clone(),
                config.checkpoint_sync_url.clone(),
                config.weak_subjectivity_checkpoint,
            )
            .await
            .expect("Unable to initialize database from checkpoint");
        }
    }

    info!("Database Initialization completed");

    let oldest_root = beacon_db
        .slot_index_provider()
        .get_oldest_root()
        .expect("Failed to access slot index provider")
        .expect("No oldest root found");
    let genesis_validators_root = beacon_db
        .state_provider()
        .get(oldest_root)
        .expect("Failed to access beacon state provider")
        .expect("No beacon state found")
        .genesis_validators_root;
    if initialize_globals {
        set_genesis_validator_root(genesis_validators_root);
    }

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

#[cfg(test)]
async fn run_beacon_node_for_test(
    config: BeaconNodeConfig,
    executor: ReamExecutor,
    ream_db: ReamDB,
) {
    run_beacon_node_inner(config, executor, ream_db, false).await;
}

/// Runs the validator node.
///
/// This function initializes the validator node by setting up the network specification,
/// loading the keystores, and creating a validator service.
/// It also starts the validator service.
pub async fn run_validator_node(config: ValidatorNodeConfig, executor: ReamExecutor) {
    run_validator_node_inner(config, executor, true).await;
}

async fn run_validator_node_inner(
    config: ValidatorNodeConfig,
    executor: ReamExecutor,
    initialize_globals: bool,
) {
    info!("starting up validator node...");

    if initialize_globals {
        set_beacon_network_spec(config.network.clone());
    }

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

#[cfg(test)]
async fn run_validator_node_for_test(config: ValidatorNodeConfig, executor: ReamExecutor) {
    run_validator_node_inner(config, executor, false).await;
}

/// Runs the account manager.
///
/// This function initializes the account manager by validating the configuration,
/// generating keys, and starting the account manager service.
pub async fn run_account_manager(config: AccountManagerConfig, ream_directory: PathBuf) {
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
                ream_directory.join(custom_path)
            }
        }
        None => ream_directory.join("keystores"),
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

        let (public_key, _private_key) = LeanSigPrivateKey::generate_key_pair_from_seed(
            seed,
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
            / beacon_network_spec().seconds_per_slot(),
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
    use std::{
        env::temp_dir,
        fs,
        ops::Range,
        path::{Path, PathBuf},
        process::{Command, Stdio},
        sync::{Arc, LazyLock, Once},
        time::{Duration, Instant, SystemTime, UNIX_EPOCH},
    };

    use alloy_primitives::{B256, fixed_bytes, hex};
    use clap::Parser;
    use libp2p_identity::{Keypair, secp256k1};
    use ream::cli::{
        Cli, Commands, beacon_node::BeaconNodeConfig, lean_node::LeanNodeConfig,
        validator_node::ValidatorNodeConfig, verbosity::Verbosity,
    };
    use ream_bls::{BLSSignature, PrivateKey, PublicKey};
    use ream_consensus_beacon::{
        data_column_sidecar::{ColumnIdentifier, NUMBER_OF_COLUMNS},
        electra::{
            beacon_block::{BeaconBlock, SignedBeaconBlock},
            beacon_block_body::BeaconBlockBody,
            beacon_state::BeaconState,
        },
        historical_summary::HistoricalSummary,
        pending_consolidation::PendingConsolidation,
        pending_deposit::PendingDeposit,
        pending_partial_withdrawal::PendingPartialWithdrawal,
        sync_committee::SyncCommittee,
    };
    use ream_consensus_lean::state::LeanState;
    use ream_consensus_misc::{
        beacon_block_header::BeaconBlockHeader,
        checkpoint::Checkpoint,
        constants::beacon::{
            FAR_FUTURE_EPOCH, MIN_ACTIVATION_BALANCE, SLOTS_PER_EPOCH,
            UNSET_DEPOSIT_REQUESTS_START_INDEX, set_genesis_validator_root,
        },
        eth_1_data::Eth1Data,
        fork::Fork,
        validator::Validator,
    };
    use ream_executor::ReamExecutor;
    use ream_fork_choice_beacon::store::get_forkchoice_store;
    use ream_keystore::{
        decrypt::aes128_ctr,
        keystore::{
            ChecksumParams, CipherParams, CryptoV4, EncryptedKeystore, FunctionBlock, KdfParams,
            Prf,
        },
    };
    use ream_mock_execution_engine::{
        MockExecutionServer, block_generator::genesis_execution_payload,
    };
    use ream_network_spec::networks::{BeaconNetworkSpec, DEV, set_beacon_network_spec};
    use ream_storage::{
        db::ReamDB,
        dir::setup_data_dir,
        tables::{
            field::REDBField,
            table::{CustomTable, REDBTable},
        },
    };
    use serde_json::Value;
    use serial_test::serial;
    use sha2::{Digest, Sha256};
    use ssz_types::{
        BitVector, FixedVector, VariableList,
        typenum::{U4, U64, U512, U8192, U65536},
    };
    use tokio::time::{sleep, timeout};
    use tracing::{info, warn};
    use tree_hash::TreeHash;

    use crate::{APP_NAME, run_beacon_node_for_test, run_lean_node, run_validator_node_for_test};

    const VALIDATOR_KEYS: [(&str, &str); 3] = [
        (
            "0x9eb14868923a923291404c6a82030e19ba0e3004b9e5b64d2419b8591657f9104298d77399350c43082a6023e812e433bfcdaa4e",
            "0xd80efa199a42987324e182419b07e4758b411d5d990f83681e0c6154f8f4af2fe5a8be4a30a25414f7f504117d95a055dc66b019",
        ),
        (
            "0x642f406d99565363657436379118ef657efd6543f2607d45e3ce565709d271339c6d9403a7d8053200b1f63c084acb6466f87b16",
            "0x789edd4c2806222f9ae35926d66c2f127192ec2d4e4cfb79e8141d6aae34a01a135f571815b7987b76c6ef3df4fa8a01fcc9785e",
        ),
        (
            "0x82d7be67ecfeec6683466605c8e2c21b217e07203b83de02fec2e62be476420c34cb64575dfa65296a01fe1df3c18557f8f8402a",
            "0xc610cc66f323a363e5ec55443e1383688a7e0a6ee9ef496b7979bf40d613e84694b3b8547690ca72e320543f3e110556c919da09",
        ),
    ];

    struct CheckpointSyncScenario {
        test_name: &'static str,
        test_duration_secs: u64,
        checkpoint_sync_start_delay: u64,
        restart_delay_after_node_3_start: Option<u64>,
        preseed_node_3_before_checkpoint_sync: bool,
    }

    const BEACON_E2E_VALIDATOR_COUNT: usize = 8;
    const BEACON_E2E_VALIDATOR_NODE_COUNT: usize = 2;
    const BEACON_E2E_SLOT_DURATION_MS: u64 = 3_000;
    const BEACON_E2E_KEYSTORE_PASSWORD: &str = "password";

    // Production's `BEACON_NETWORK_SPEC`/`GENESIS_VALIDATORS_ROOT` OnceLocks only allow one set
    // per process. Every beacon e2e test below therefore uses this shared dev spec + genesis.
    static BEACON_E2E_DEV_SPEC: LazyLock<Arc<BeaconNetworkSpec>> = LazyLock::new(|| {
        let mut spec = (**DEV).clone();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("System time is before UNIX epoch")
            .as_secs();
        let seconds_per_slot = BEACON_E2E_SLOT_DURATION_MS / 1000;
        spec.min_genesis_time = now - (now % seconds_per_slot) - seconds_per_slot * 2;
        spec.genesis_delay = 0;
        spec.altair_fork_epoch = 0;
        spec.bellatrix_fork_epoch = 0;
        spec.capella_fork_epoch = 0;
        spec.deneb_fork_epoch = 0;
        spec.electra_fork_epoch = 0;
        spec.fulu_fork_epoch = FAR_FUTURE_EPOCH;
        spec.slot_duration_ms = BEACON_E2E_SLOT_DURATION_MS;
        Arc::new(spec)
    });
    static BEACON_E2E_NETWORK_SPEC_INIT: Once = Once::new();
    static BEACON_E2E_GENESIS_ROOT_INIT: Once = Once::new();

    fn init_test_tracing() {
        if let Err(err) = tracing_subscriber::fmt()
            .with_env_filter(Verbosity::Info.directive())
            .with_test_writer()
            .try_init()
        {
            warn!("Failed to initialize tracing subscriber: {err}");
        }
    }

    fn lean_assets_directory() -> PathBuf {
        [
            PathBuf::from("bin/ream/assets/lean"),
            PathBuf::from("assets/lean"),
            PathBuf::from("../assets/lean"),
        ]
        .into_iter()
        .find(|p| p.exists())
        .expect("Could not find 'assets/lean' directory.")
        .canonicalize()
        .expect("Failed to canonicalize assets path")
    }

    fn write_test_validator_registry(
        assets_directory: &Path,
        test_name: &str,
        node_count: usize,
    ) -> PathBuf {
        let registry_path =
            assets_directory.join(format!("test_multi_node_registry_{test_name}.yaml"));

        let mut validators_yaml = String::new();
        for (i, keys) in VALIDATOR_KEYS.iter().enumerate().take(node_count) {
            validators_yaml.push_str(&format!("node{}:\n", i + 1));
            let (attester_key, proposer_key) = keys;
            validators_yaml.push_str(&format!(
                "  - index: {i}\n    pubkey_hex: {attester_key}\n    privkey_file: validator_{i}_attestation_sk.ssz\n"
            ));
            validators_yaml.push_str(&format!(
                "  - index: {i}\n    pubkey_hex: {proposer_key}\n    privkey_file: validator_{i}_proposal_sk.ssz\n"
            ));
        }

        fs::write(&registry_path, validators_yaml).expect("Failed to write temp registry");
        registry_path
    }

    fn spawn_lean_test_node(
        config: LeanNodeConfig,
        db: ReamDB,
        executor: ReamExecutor,
    ) -> tokio::task::JoinHandle<()> {
        use tracing::{Instrument, info_span};

        let span = info_span!("lean_node", node_id = %config.node_id);
        tokio::spawn(
            async move {
                run_lean_node(config, executor, db).await;
            }
            .instrument(span),
        )
    }

    fn spawn_beacon_test_node(
        config: BeaconNodeConfig,
        db: ReamDB,
        executor: ReamExecutor,
    ) -> tokio::task::JoinHandle<()> {
        use tracing::{Instrument, info_span};

        let span = info_span!(
            "beacon_node",
            http_port = config.http_port,
            socket_port = config.socket_port,
            discovery_port = config.discovery_port,
        );
        tokio::spawn(
            async move {
                run_beacon_node_for_test(config, executor, db).await;
            }
            .instrument(span),
        )
    }

    fn beacon_e2e_dev_spec() -> Arc<BeaconNetworkSpec> {
        BEACON_E2E_DEV_SPEC.clone()
    }

    fn initialize_beacon_e2e_network_spec(network_spec: Arc<BeaconNetworkSpec>) {
        BEACON_E2E_NETWORK_SPEC_INIT.call_once(|| {
            set_beacon_network_spec(network_spec);
        });
    }

    fn initialize_beacon_e2e_genesis_root(genesis_validators_root: B256) {
        BEACON_E2E_GENESIS_ROOT_INIT.call_once(|| {
            set_genesis_validator_root(genesis_validators_root);
        });
    }

    fn indexed_private_key(index: usize) -> B256 {
        let mut bytes = [0u8; 32];
        bytes[24..].copy_from_slice(&(index as u64 + 1).to_be_bytes());
        B256::from(bytes)
    }

    fn public_key_from_private_key(private_key: B256) -> PublicKey {
        PrivateKey { inner: private_key }
            .public_key()
            .expect("test private key should derive a valid BLS public key")
    }

    fn beacon_e2e_public_keys() -> Vec<PublicKey> {
        (0..BEACON_E2E_VALIDATOR_COUNT)
            .map(|index| public_key_from_private_key(indexed_private_key(index)))
            .collect()
    }

    fn build_dev_genesis(pubkeys: &[PublicKey]) -> (BeaconState, SignedBeaconBlock) {
        let genesis_time = beacon_e2e_dev_spec().min_genesis_time;
        let validators = pubkeys
            .iter()
            .cloned()
            .map(|public_key| {
                let mut withdrawal_credentials = [0u8; 32];
                withdrawal_credentials[0] = 1;
                Validator {
                    public_key,
                    withdrawal_credentials: B256::from(withdrawal_credentials),
                    effective_balance: MIN_ACTIVATION_BALANCE,
                    slashed: false,
                    activation_eligibility_epoch: 0,
                    activation_epoch: 0,
                    exit_epoch: FAR_FUTURE_EPOCH,
                    withdrawable_epoch: FAR_FUTURE_EPOCH,
                }
            })
            .collect::<Vec<_>>();
        let validators =
            VariableList::new(validators).expect("test validator set should fit registry limit");
        let genesis_validators_root = validators.tree_hash_root();
        let balances = VariableList::new(vec![MIN_ACTIVATION_BALANCE; pubkeys.len()])
            .expect("test balances should fit registry limit");
        let participation = VariableList::new(vec![0u8; pubkeys.len()])
            .expect("test participation should fit registry limit");
        let inactivity_scores =
            VariableList::new(vec![0u64; pubkeys.len()]).expect("test scores should fit limit");
        let sync_public_keys = FixedVector::<PublicKey, U512>::try_from(
            (0..512)
                .map(|index| pubkeys[index % pubkeys.len()].clone())
                .collect::<Vec<_>>(),
        )
        .expect("sync committee should have 512 pubkeys");
        let sync_committee = Arc::new(SyncCommittee {
            public_keys: sync_public_keys,
            aggregate_public_key: pubkeys[0].clone(),
        });
        let genesis_payload = genesis_execution_payload(B256::ZERO, genesis_time);
        let genesis_body = BeaconBlockBody {
            execution_payload: genesis_payload.clone(),
            ..Default::default()
        };
        let state = BeaconState {
            genesis_time,
            genesis_validators_root,
            slot: 0,
            fork: Fork {
                previous_version: fixed_bytes!("0x05000000"),
                current_version: fixed_bytes!("0x05000000"),
                epoch: 0,
            },
            latest_block_header: BeaconBlockHeader {
                body_root: genesis_body.tree_hash_root(),
                ..Default::default()
            },
            block_roots: FixedVector::<B256, U8192>::from_elem(B256::ZERO),
            state_roots: FixedVector::<B256, U8192>::from_elem(B256::ZERO),
            historical_roots: VariableList::empty(),
            eth1_data: Eth1Data::default(),
            eth1_data_votes: VariableList::empty(),
            eth1_deposit_index: 0,
            validators,
            balances,
            randao_mixes: FixedVector::<B256, U65536>::from_elem(B256::ZERO),
            slashings: FixedVector::<u64, U8192>::from_elem(0),
            previous_epoch_participation: participation.clone(),
            current_epoch_participation: participation,
            justification_bits: BitVector::<U4>::default(),
            previous_justified_checkpoint: Checkpoint::default(),
            current_justified_checkpoint: Checkpoint::default(),
            finalized_checkpoint: Checkpoint::default(),
            inactivity_scores,
            current_sync_committee: sync_committee.clone(),
            next_sync_committee: sync_committee,
            latest_execution_payload_header: genesis_payload.to_execution_payload_header(),
            next_withdrawal_index: 0,
            next_withdrawal_validator_index: 0,
            historical_summaries: VariableList::<HistoricalSummary, _>::empty(),
            deposit_requests_start_index: UNSET_DEPOSIT_REQUESTS_START_INDEX,
            deposit_balance_to_consume: 0,
            exit_balance_to_consume: 0,
            earliest_exit_epoch: FAR_FUTURE_EPOCH,
            consolidation_balance_to_consume: 0,
            earliest_consolidation_epoch: FAR_FUTURE_EPOCH,
            pending_deposits: VariableList::<PendingDeposit, _>::empty(),
            pending_partial_withdrawals: VariableList::<PendingPartialWithdrawal, _>::empty(),
            pending_consolidations: VariableList::<PendingConsolidation, _>::empty(),
            proposer_lookahead: FixedVector::<u64, U64>::from_elem(0),
        };

        let mut genesis_block = SignedBeaconBlock {
            message: BeaconBlock {
                body: genesis_body,
                ..Default::default()
            },
            signature: BLSSignature::default(),
        };
        genesis_block.message.state_root = state.tree_hash_root();
        (state, genesis_block)
    }

    fn seed_beacon_test_db(
        db: &ReamDB,
        genesis_state: BeaconState,
        genesis_block: &SignedBeaconBlock,
    ) -> B256 {
        let genesis_validators_root = genesis_state.genesis_validators_root;
        let beacon_db = db
            .init_beacon_db()
            .expect("unable to init Ream Beacon Database");

        let _store = get_forkchoice_store(genesis_state, genesis_block.message.clone(), beacon_db)
            .expect("Failed to seed Beacon DB from fixture");

        genesis_validators_root
    }

    fn validator_key_ranges(validator_node_count: usize) -> Vec<Range<usize>> {
        assert!(
            validator_node_count > 0 && validator_node_count <= BEACON_E2E_VALIDATOR_COUNT,
            "validator node count must be between 1 and {BEACON_E2E_VALIDATOR_COUNT}"
        );

        let base_keys_per_node = BEACON_E2E_VALIDATOR_COUNT / validator_node_count;
        let remainder = BEACON_E2E_VALIDATOR_COUNT % validator_node_count;
        let mut start = 0;

        (0..validator_node_count)
            .map(|node_index| {
                let key_count = base_keys_per_node + usize::from(node_index < remainder);
                let end = start + key_count;
                let range = start..end;
                start = end;
                range
            })
            .collect()
    }

    fn write_validator_keystores(
        test_dir: &Path,
        validator_node_index: usize,
        validator_indices: Range<usize>,
    ) -> (PathBuf, PathBuf) {
        assert!(
            !validator_indices.is_empty(),
            "validator node {validator_node_index} must have at least one key"
        );
        let keystore_dir = test_dir.join(format!("validators_{validator_node_index}"));
        fs::create_dir_all(&keystore_dir).expect("Failed to create keystore directory");
        let password_file = test_dir.join(format!("password_{validator_node_index}.txt"));
        fs::write(&password_file, BEACON_E2E_KEYSTORE_PASSWORD)
            .expect("Failed to write keystore password file");

        for index in validator_indices {
            assert!(
                index < BEACON_E2E_VALIDATOR_COUNT,
                "validator index {index} is outside the e2e validator set"
            );
            let private_key = indexed_private_key(index);
            let public_key = public_key_from_private_key(private_key);
            let salt = B256::from_slice(&Sha256::digest(index.to_be_bytes()));
            let iv = Sha256::digest((index as u64 + 10_000).to_be_bytes())[..16].to_vec();
            let kdf_params = KdfParams::Pbkdf2 {
                c: 2,
                dklen: 32,
                prf: Prf::HmacSha256,
                salt: salt.to_vec(),
            };
            let derived_key = kdf_params
                .derive_key(BEACON_E2E_KEYSTORE_PASSWORD.as_bytes())
                .expect("test PBKDF2 parameters should derive a key");
            let mut cipher_message = private_key.to_vec();
            aes128_ctr(
                &mut cipher_message,
                derived_key[..16]
                    .try_into()
                    .expect("derived key slice should be 16 bytes"),
                iv.as_slice().try_into().expect("iv should be 16 bytes"),
            );
            let checksum = Sha256::digest([&derived_key[16..32], &cipher_message].concat());
            let keystore = EncryptedKeystore {
                crypto: CryptoV4 {
                    kdf: FunctionBlock {
                        params: kdf_params,
                        message: vec![],
                    },
                    checksum: FunctionBlock {
                        params: ChecksumParams::Sha256 {},
                        message: checksum.to_vec(),
                    },
                    cipher: FunctionBlock {
                        params: CipherParams::Aes128Ctr { iv },
                        message: cipher_message,
                    },
                },
                description: format!("Ream beacon e2e validator {index}"),
                public_key,
                path: format!("m/12381/3600/{index}/0/0"),
                uuid: format!("00000000-0000-0000-0000-{index:012}"),
                version: 4,
            };
            keystore
                .save_to_file(keystore_dir.join(format!("validator_{index}.json")))
                .expect("Failed to save test keystore");
        }

        (keystore_dir, password_file)
    }

    fn beacon_node_config_from_args(
        data_port_offset: u16,
        bootnodes: Option<String>,
    ) -> BeaconNodeConfig {
        let http_port = 26652 + data_port_offset;
        let socket_port = 30600 + data_port_offset;
        let discovery_port = 31600 + data_port_offset;
        let args = vec![
            "ream".to_string(),
            "beacon_node".to_string(),
            "--network".to_string(),
            "sepolia".to_string(),
            "--http-address".to_string(),
            "127.0.0.1".to_string(),
            "--http-port".to_string(),
            http_port.to_string(),
            "--socket-address".to_string(),
            "127.0.0.1".to_string(),
            "--socket-port".to_string(),
            socket_port.to_string(),
            "--discovery-port".to_string(),
            discovery_port.to_string(),
            "--bootnodes".to_string(),
            bootnodes.unwrap_or_else(|| "none".to_string()),
        ];

        let cli = Cli::parse_from(args);
        let Commands::BeaconNode(config) = cli.command else {
            panic!("Expected beacon_node command");
        };
        let mut config = *config;
        config.network = beacon_e2e_dev_spec();
        config
    }

    async fn find_slot_with_blob_commitments(
        http_port: u16,
        max_slot: u64,
    ) -> Option<(u64, serde_json::Value)> {
        let client = reqwest::Client::new();
        for slot in 1..=max_slot {
            let url = format!("http://127.0.0.1:{http_port}/eth/v2/beacon/blocks/{slot}");
            let Ok(response) = client.get(&url).send().await else {
                continue;
            };
            if !response.status().is_success() {
                continue;
            }
            let Ok(body) = response.json::<serde_json::Value>().await else {
                continue;
            };
            let commitment_count = body["data"]["message"]["body"]["blob_kzg_commitments"]
                .as_array()
                .map(Vec::len)
                .unwrap_or(0);
            if commitment_count > 0 {
                return Some((slot, body));
            }
        }
        None
    }

    async fn wait_for_beacon_json(http_port: u16, path: &str) -> serde_json::Value {
        let client = reqwest::Client::new();
        let url = format!("http://127.0.0.1:{http_port}{path}");
        let start = Instant::now();
        let timeout_duration = Duration::from_secs(20);

        loop {
            match client.get(&url).send().await {
                Ok(response) if response.status().is_success() => {
                    return response
                        .json::<serde_json::Value>()
                        .await
                        .expect("Failed to decode Beacon API response");
                }
                Ok(response) => {
                    info!(status = %response.status(), url, "Beacon API endpoint not ready");
                }
                Err(err) => {
                    info!(%err, url, "Beacon API endpoint not ready");
                }
            }

            assert!(
                start.elapsed() < timeout_duration,
                "Timed out waiting for Beacon API endpoint {url}"
            );
            sleep(Duration::from_millis(250)).await;
        }
    }

    async fn wait_for_beacon_identity(http_port: u16) -> serde_json::Value {
        let identity = wait_for_beacon_json(http_port, "/eth/v1/node/identity").await;
        assert!(
            identity["data"]["peer_id"]
                .as_str()
                .is_some_and(|peer_id| !peer_id.is_empty()),
            "Beacon node identity response did not include a peer_id: {identity:?}"
        );
        assert!(
            identity["data"]["enr"]
                .as_str()
                .is_some_and(|enr| !enr.is_empty()),
            "Beacon node identity response did not include an ENR: {identity:?}"
        );
        identity
    }

    fn peer_count_value(peer_count: &serde_json::Value, field: &str) -> u64 {
        let value = &peer_count["data"][field];
        value
            .as_u64()
            .or_else(|| value.as_str().and_then(|value| value.parse().ok()))
            .unwrap_or_else(|| {
                panic!("Beacon peer_count response did not include {field} count: {peer_count:?}")
            })
    }

    fn peer_count_connected(peer_count: &serde_json::Value) -> u64 {
        peer_count_value(peer_count, "connected")
    }

    fn peer_count_total(peer_count: &serde_json::Value) -> u64 {
        peer_count_value(peer_count, "connected")
            + peer_count_value(peer_count, "connecting")
            + peer_count_value(peer_count, "disconnected")
            + peer_count_value(peer_count, "disconnecting")
    }

    async fn wait_for_connected_beacon_peer(
        http_ports: &[u16],
    ) -> Result<Vec<serde_json::Value>, Vec<serde_json::Value>> {
        let start = Instant::now();
        let timeout_duration = Duration::from_secs(60);
        loop {
            let mut peer_counts = Vec::new();
            for http_port in http_ports {
                peer_counts.push(wait_for_beacon_json(*http_port, "/eth/v1/node/peer_count").await);
            }

            let every_node_knows_a_peer =
                peer_counts.iter().all(|count| peer_count_total(count) > 0);
            let any_node_connected = peer_counts
                .iter()
                .any(|count| peer_count_connected(count) > 0);
            if every_node_knows_a_peer && any_node_connected {
                return Ok(peer_counts);
            }

            if start.elapsed() >= timeout_duration {
                let err = peer_counts;
                return Err(err);
            }
            sleep(Duration::from_millis(500)).await;
        }
    }

    async fn shutdown_beacon_test_node(
        executor: &ReamExecutor,
        handle: tokio::task::JoinHandle<()>,
    ) {
        executor.shutdown_signal();
        let mut handle = handle;
        tokio::select! {
            _ = &mut handle => {},
            _ = sleep(Duration::from_secs(5)) => {
                warn!("Timed out waiting for beacon node test task to shut down");
                handle.abort();
            }
        }
    }

    fn validator_node_config_from_args(
        beacon_http_port: u16,
        keystore_dir: &Path,
        password_file: &Path,
    ) -> ValidatorNodeConfig {
        let cli = Cli::parse_from([
            "ream",
            "validator_node",
            "--network",
            "dev",
            "--beacon-api-endpoint",
            &format!("http://127.0.0.1:{beacon_http_port}"),
            "--request-timeout",
            "3",
            "--import-keystores",
            &keystore_dir.to_string_lossy(),
            "--suggested-fee-recipient",
            "0x0000000000000000000000000000000000000001",
            "--password-file",
            &password_file.to_string_lossy(),
        ]);
        let Commands::ValidatorNode(config) = cli.command else {
            panic!("Expected validator_node command");
        };
        let mut config = *config;
        config.network = beacon_e2e_dev_spec();
        config
    }

    fn spawn_validator_test_node(
        config: ValidatorNodeConfig,
        executor: ReamExecutor,
    ) -> tokio::task::JoinHandle<anyhow::Result<()>> {
        use tracing::{Instrument, info_span};

        let span = info_span!("validator_node", beacon_api_endpoint = %config.beacon_api_endpoint);
        let executor_for_task = executor.clone();
        executor.spawn(
            async move {
                run_validator_node_for_test(config, executor_for_task).await;
            }
            .instrument(span),
        )
    }

    fn spawn_validator_test_nodes(
        beacon_http_ports: &[u16],
        test_dir: &Path,
        executors: &[ReamExecutor],
    ) -> Vec<tokio::task::JoinHandle<anyhow::Result<()>>> {
        assert!(
            !beacon_http_ports.is_empty(),
            "at least one beacon API endpoint is required"
        );
        let validator_ranges = validator_key_ranges(executors.len());

        executors
            .iter()
            .enumerate()
            .zip(validator_ranges)
            .map(|((validator_node_index, executor), validator_indices)| {
                let (keystore_dir, password_file) =
                    write_validator_keystores(test_dir, validator_node_index, validator_indices);
                let beacon_http_port =
                    beacon_http_ports[validator_node_index % beacon_http_ports.len()];
                let config = validator_node_config_from_args(
                    beacon_http_port,
                    &keystore_dir,
                    &password_file,
                );
                spawn_validator_test_node(config, executor.clone())
            })
            .collect()
    }

    async fn shutdown_validator_test_node(
        executor: &ReamExecutor,
        handle: tokio::task::JoinHandle<anyhow::Result<()>>,
    ) {
        executor.shutdown_signal();
        let mut handle = handle;
        tokio::select! {
            _ = &mut handle => {},
            _ = sleep(Duration::from_secs(5)) => {
                warn!("Timed out waiting for validator node test task to shut down");
                handle.abort();
            }
        }
    }

    async fn shutdown_validator_test_nodes(
        executors: &[ReamExecutor],
        handles: Vec<tokio::task::JoinHandle<anyhow::Result<()>>>,
    ) {
        assert_eq!(
            executors.len(),
            handles.len(),
            "validator executors and handles must match"
        );

        for (executor, handle) in executors.iter().zip(handles) {
            shutdown_validator_test_node(executor, handle).await;
        }
    }

    fn head_slot_and_root(head: &Value) -> (u64, String) {
        let slot = head["data"]["header"]["message"]["slot"]
            .as_str()
            .and_then(|slot| slot.parse::<u64>().ok())
            .or_else(|| head["data"]["header"]["message"]["slot"].as_u64())
            .unwrap_or_else(|| panic!("head response missing slot: {head:?}"));
        let root = head["data"]["root"]
            .as_str()
            .unwrap_or_else(|| panic!("head response missing root: {head:?}"))
            .to_string();
        (slot, root)
    }

    #[derive(Debug, Clone)]
    struct BeaconNodeStatus {
        http_port: u16,
        head_slot: u64,
        head_root: String,
        previous_justified_epoch: u64,
        justified_epoch: u64,
        finalized_epoch: u64,
    }

    async fn beacon_node_status(http_port: u16) -> BeaconNodeStatus {
        let head = wait_for_beacon_json(http_port, "/eth/v1/beacon/headers").await;
        let (head_slot, head_root) = head_slot_and_root(&head);
        let checkpoints =
            wait_for_beacon_json(http_port, "/eth/v1/beacon/states/head/finality_checkpoints")
                .await;

        BeaconNodeStatus {
            http_port,
            head_slot,
            head_root,
            previous_justified_epoch: checkpoint_epoch(&checkpoints, "previous_justified"),
            justified_epoch: checkpoint_epoch(&checkpoints, "current_justified"),
            finalized_epoch: checkpoint_epoch(&checkpoints, "finalized"),
        }
    }

    async fn beacon_node_statuses(http_ports: &[u16]) -> Vec<BeaconNodeStatus> {
        let mut statuses = Vec::with_capacity(http_ports.len());
        for http_port in http_ports {
            statuses.push(beacon_node_status(*http_port).await);
        }
        statuses
    }

    async fn wait_for_all_data_column_sidecars(db: &ReamDB, block_root: B256) {
        let beacon_db = db
            .init_beacon_db()
            .expect("unable to init Ream Beacon Database for inspection");
        let column_sidecars_provider = beacon_db.column_sidecars_provider();

        let start = Instant::now();
        let timeout_duration = Duration::from_secs(30);
        loop {
            let missing_index = (0..NUMBER_OF_COLUMNS).find(|&index| {
                column_sidecars_provider
                    .get(ColumnIdentifier::new(block_root, index))
                    .expect("column sidecar lookup should not error")
                    .is_none()
            });
            let Some(missing_index) = missing_index else {
                return;
            };

            assert!(
                start.elapsed() < timeout_duration,
                "timed out waiting for data column sidecar {missing_index} for block {block_root:?}"
            );
            sleep(Duration::from_millis(250)).await;
        }
    }

    async fn wait_for_head_slot_at_least(http_port: u16, min_slot: u64) -> (u64, String) {
        let start = Instant::now();
        let timeout_duration = Duration::from_secs(300);
        loop {
            let head = wait_for_beacon_json(http_port, "/eth/v1/beacon/headers").await;
            let (slot, root) = head_slot_and_root(&head);
            if slot >= min_slot {
                return (slot, root);
            }

            assert!(
                start.elapsed() < timeout_duration,
                "Timed out waiting for beacon head on port {http_port} to reach slot {min_slot}; latest slot {slot}"
            );
            sleep(Duration::from_millis(500)).await;
        }
    }

    async fn wait_for_matching_heads_all(http_ports: &[u16]) -> (u64, String) {
        assert!(
            !http_ports.is_empty(),
            "need at least one beacon node to compare heads"
        );
        let start = Instant::now();
        let timeout_duration = Duration::from_secs(60);
        loop {
            let statuses = beacon_node_statuses(http_ports).await;
            let status_heads = statuses
                .iter()
                .map(|status| {
                    (
                        status.http_port,
                        status.head_slot,
                        status.head_root.as_str(),
                    )
                })
                .collect::<Vec<_>>();
            info!(?status_heads, "beacon convergence poll");

            let first_status = &statuses[0];
            if first_status.head_slot > 0
                && statuses.iter().all(|status| {
                    status.head_slot == first_status.head_slot
                        && status.head_root == first_status.head_root
                })
            {
                return (first_status.head_slot, first_status.head_root.clone());
            }

            assert!(
                start.elapsed() < timeout_duration,
                "Timed out waiting for matching beacon heads: {statuses:?}"
            );
            sleep(Duration::from_secs(1)).await;
        }
    }

    fn checkpoint_epoch(checkpoints: &Value, checkpoint_kind: &str) -> u64 {
        checkpoints["data"][checkpoint_kind]["epoch"]
            .as_str()
            .and_then(|epoch| epoch.parse::<u64>().ok())
            .or_else(|| checkpoints["data"][checkpoint_kind]["epoch"].as_u64())
            .unwrap_or_else(|| {
                panic!(
                    "finality checkpoint response missing {checkpoint_kind} epoch: {checkpoints:?}"
                )
            })
    }

    async fn wait_for_finality_checkpoints_advanced_all(
        http_ports: &[u16],
    ) -> Vec<BeaconNodeStatus> {
        assert!(
            !http_ports.is_empty(),
            "need at least one beacon node to check finality"
        );
        let start = Instant::now();
        let timeout_duration = Duration::from_secs(300);
        loop {
            let statuses = beacon_node_statuses(http_ports).await;
            let checkpoint_epochs = statuses
                .iter()
                .map(|status| {
                    (
                        status.http_port,
                        status.head_slot,
                        status.previous_justified_epoch,
                        status.justified_epoch,
                        status.finalized_epoch,
                    )
                })
                .collect::<Vec<_>>();
            info!(?checkpoint_epochs, "beacon finality poll");

            if statuses
                .iter()
                .all(|status| status.justified_epoch > 0 && status.finalized_epoch > 0)
            {
                return statuses;
            }

            assert!(
                start.elapsed() < timeout_duration,
                "Timed out waiting for beacon finality: {statuses:?}"
            );
            sleep(Duration::from_secs(1)).await;
        }
    }

    fn read_head_state(db: &ReamDB) -> Option<LeanState> {
        let lean_db = db.init_lean_db().ok()?;
        let head = lean_db.head_provider().get().ok()?;
        lean_db.state_provider().get(head).ok().flatten()
    }

    fn run_checkpoint_sync_scenario(scenario: CheckpointSyncScenario) {
        init_test_tracing();

        info!("Starting checkpoint sync test: {}", scenario.test_name);

        let base_p2p_port = 23600;
        let base_http_port = 19652;
        let node_count = 3;
        let port_offset = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("System time is before UNIX epoch")
            .subsec_nanos() as u16
            % 1000;
        let assets_directory = lean_assets_directory();
        let registry_path =
            write_test_validator_registry(&assets_directory, scenario.test_name, node_count);
        let registry_path_string = registry_path.to_string_lossy().to_string();

        let network_config_path = create_test_network_config(scenario.test_name, 3);
        let network_config_path_string = network_config_path.to_string_lossy().to_string();

        let control_executor = ReamExecutor::new().unwrap();
        let (remaining_node_executors, retired_node_executors) =
            control_executor.clone().runtime().block_on(async move {
            let mut node_addresses: Vec<String> = Vec::new();
            let mut db_instances: Vec<Option<ReamDB>> = vec![None; node_count];
            let mut node_executors: Vec<Option<ReamExecutor>> = vec![None; node_count];
            let mut retired_node_executors = Vec::new();
            let mut key_paths = Vec::new();

            for (i, db_slot) in db_instances.iter_mut().enumerate().take(node_count) {
                let node_index = i + 1;
                let ream_directory = temp_dir()
                    .join(format!("{APP_NAME}_{}_node_{node_index}", scenario.test_name));

                if ream_directory.exists()
                    && let Err(err) = fs::remove_dir_all(&ream_directory)
                {
                    warn!("Failed to remove ream directory: {err}");
                }
                fs::create_dir_all(&ream_directory).expect("Failed to create data directory");

                let key_path = ream_directory.join("node_key");
                let peer_id = generate_node_identity(&key_path);
                key_paths.push(key_path);

                let p2p_port = base_p2p_port + port_offset + (i as u16);

                let address =
                    format!("/ip4/127.0.0.1/udp/{p2p_port}/quic-v1/p2p/{peer_id}");
                node_addresses.push(address);

                *db_slot = Some(ReamDB::new(ream_directory).unwrap());
            }

            let node_1_http_port = base_http_port + port_offset;
            let mut node_handles: Vec<Option<tokio::task::JoinHandle<()>>> =
                (0..node_count).map(|_| None).collect();

            for i in 0..2 {
                let node_index = i + 1;
                let p2p_port = base_p2p_port + port_offset + (i as u16);
                let http_port = base_http_port + port_offset + (i as u16);

                let mut args = vec![
                    "ream".to_string(),
                    "lean_node".to_string(),
                    "--network".to_string(),
                    network_config_path_string.clone(),
                    "--validator-registry-path".to_string(),
                    registry_path_string.clone(),
                    "--socket-port".to_string(),
                    p2p_port.to_string(),
                    "--socket-address".to_string(),
                    "127.0.0.1".to_string(),
                    "--http-port".to_string(),
                    http_port.to_string(),
                    "--node-id".to_string(),
                    format!("node{node_index}"),
                    "--private-key-path".to_string(),
                    key_paths[i].to_string_lossy().to_string(),
                ];

                if i == 0 {
                    args.push("--is-aggregator".to_string());
                } else {
                    args.push("--bootnodes".to_string());
                    args.push(node_addresses[0].clone());
                }

                let cli = Cli::parse_from(args);
                let Commands::LeanNode(config) = cli.command else {
                    panic!("Expected lean_node command");
                };

                let db = db_instances[i].clone().unwrap();
                let node_executor = ReamExecutor::new().unwrap();
                let handle = spawn_lean_test_node(*config, db, node_executor.clone());
                node_executors[i] = Some(node_executor);
                node_handles[i] = Some(handle);

                if i == 0 {
                    info!("Waiting 5s for Node 1 to initialize QUIC listener...");
                    sleep(Duration::from_secs(5)).await;
                }
            }

            info!("Nodes 1 and 2 started, monitoring checkpoint sync scenarios...");

            let start_time = Instant::now();
            let mut node_3_started = false;
            let mut node_3_checkpoint_sync_started = false;
            let mut node_3_restarted = false;
            let mut node_3_config: Option<LeanNodeConfig> = None;
            let mut node_3_state_before_restart: Option<LeanState> = None;

            if scenario.preseed_node_3_before_checkpoint_sync {
                info!("Starting Node 3 without checkpoint sync to create existing local state...");

                let node_3_p2p_port = base_p2p_port + port_offset + 2;
                let node_3_http_port = base_http_port + port_offset + 2;

                let node_3_args = vec![
                    "ream".to_string(),
                    "lean_node".to_string(),
                    "--network".to_string(),
                    network_config_path_string.clone(),
                    "--validator-registry-path".to_string(),
                    registry_path_string.clone(),
                    "--socket-port".to_string(),
                    node_3_p2p_port.to_string(),
                    "--socket-address".to_string(),
                    "127.0.0.1".to_string(),
                    "--http-port".to_string(),
                    node_3_http_port.to_string(),
                    "--node-id".to_string(),
                    "node3".to_string(),
                    "--private-key-path".to_string(),
                    key_paths[2].to_string_lossy().to_string(),
                    "--bootnodes".to_string(),
                    format!("{},{}", node_addresses[0], node_addresses[1]),
                ];

                let cli_3 = Cli::parse_from(node_3_args);
                let Commands::LeanNode(config_3) = cli_3.command else {
                    panic!("Expected lean_node command");
                };

                let db_3 = db_instances[2].clone().unwrap();
                let node_executor = ReamExecutor::new().unwrap();
                node_handles[2] = Some(spawn_lean_test_node(*config_3, db_3, node_executor.clone()));
                node_executors[2] = Some(node_executor);
                node_3_started = true;
            }

            loop {
                let elapsed = start_time.elapsed().as_secs();
                if elapsed >= scenario.test_duration_secs {
                    break;
                }

                if !node_3_checkpoint_sync_started
                    && elapsed >= scenario.checkpoint_sync_start_delay
                {
                    if node_3_started {
                        info!(
                            "Restarting Node 3 with --checkpoint-sync-url using existing data directory..."
                        );
                        node_3_state_before_restart =
                            db_instances[2].as_ref().and_then(read_head_state);

                        if let Some(node_executor) = node_executors[2].take() {
                            node_executor.shutdown_signal();
                            retired_node_executors.push(node_executor);
                        }

                        if let Some(handle) = node_handles[2].take() {
                            let _ = timeout(Duration::from_secs(5), handle).await;
                        }

                        sleep(Duration::from_secs(2)).await;
                    } else {
                        info!("Starting Node 3 with --checkpoint-sync-url from Node 1...");
                    }

                    let node_3_p2p_port = base_p2p_port + port_offset + 2;
                    let node_3_http_port = base_http_port + port_offset + 2;

                    let node_3_args = vec![
                        "ream".to_string(),
                        "lean_node".to_string(),
                        "--network".to_string(),
                        network_config_path_string.clone(),
                        "--validator-registry-path".to_string(),
                        registry_path_string.clone(),
                        "--socket-port".to_string(),
                        node_3_p2p_port.to_string(),
                        "--socket-address".to_string(),
                        "127.0.0.1".to_string(),
                        "--http-port".to_string(),
                        node_3_http_port.to_string(),
                        "--node-id".to_string(),
                        "node3".to_string(),
                        "--private-key-path".to_string(),
                        key_paths[2].to_string_lossy().to_string(),
                        "--checkpoint-sync-url".to_string(),
                        format!("http://127.0.0.1:{node_1_http_port}"),
                        "--bootnodes".to_string(),
                        format!("{},{}", node_addresses[0], node_addresses[1]),
                    ];

                    let cli_3 = Cli::parse_from(node_3_args);
                    let Commands::LeanNode(config_3) = cli_3.command else {
                        panic!("Expected lean_node command");
                    };

                    let mut config_3 = *config_3;
                    if node_3_started {
                        config_3.socket_port += 50;
                        config_3.http_port += 50;
                    }
                    let db_3 = db_instances[2].clone().unwrap();
                    let node_executor = ReamExecutor::new().unwrap();
                    node_3_config = Some(config_3.clone());
                    node_handles[2] =
                        Some(spawn_lean_test_node(config_3, db_3, node_executor.clone()));
                    node_executors[2] = Some(node_executor);
                    node_3_started = true;
                    node_3_checkpoint_sync_started = true;
                }

                if node_3_checkpoint_sync_started
                    && !node_3_restarted
                    && let Some(restart_delay) = scenario.restart_delay_after_node_3_start
                    && elapsed >= scenario.checkpoint_sync_start_delay + restart_delay
                {
                    info!("Restarting Node 3 on existing data directory...");

                    node_3_state_before_restart =
                        db_instances[2].as_ref().and_then(read_head_state);

                    if let Some(node_executor) = node_executors[2].take() {
                        node_executor.shutdown_signal();
                        retired_node_executors.push(node_executor);
                    }

                    if let Some(handle) = node_handles[2].take() {
                        let _ = timeout(Duration::from_secs(5), handle).await;
                    }

                    sleep(Duration::from_secs(2)).await;

                    let mut config_3 = node_3_config
                        .clone()
                        .expect("Node 3 config should exist before restart");
                    config_3.socket_port += 50;
                    config_3.http_port += 50;
                    let db_3 = db_instances[2].clone().unwrap();
                    let node_executor = ReamExecutor::new().unwrap();
                    node_handles[2] =
                        Some(spawn_lean_test_node(config_3, db_3, node_executor.clone()));
                    node_executors[2] = Some(node_executor);
                    node_3_restarted = true;
                }

                sleep(Duration::from_secs(2)).await;

                for (i, db_option) in db_instances.iter().enumerate() {
                    if let Some(db) = db_option
                        && let Some(state) = read_head_state(db)
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

            if let Err(err) = fs::remove_file(&registry_path) {
                warn!("Failed to remove registry file: {err}");
            }
            if let Err(err) = fs::remove_file(&network_config_path) {
                warn!("Failed to remove network config file: {err}");
            }
            for node_executor in node_executors.iter().flatten() {
                node_executor.shutdown_signal();
            }
            for handle in node_handles.into_iter().flatten() {
                let _ = timeout(Duration::from_secs(5), handle).await;
            }
            sleep(Duration::from_secs(2)).await;

            let head_state_3 = read_head_state(db_instances[2].as_ref().unwrap())
                .expect("Failed to get head state for node 3");
            let head_state_1 = read_head_state(db_instances[0].as_ref().unwrap())
                .expect("Failed to get head state for node 1");

            info!(
                "FINAL: Node 1 Slot: {}, Finalized: {} | Node 3 Slot: {}, Finalized: {}",
                head_state_1.slot,
                head_state_1.latest_finalized.slot,
                head_state_3.slot,
                head_state_3.latest_finalized.slot,
            );

            assert!(
                head_state_3.latest_finalized.slot > 0,
                "Checkpoint-synced node failed to finalize. Finalized slot: {}",
                head_state_3.latest_finalized.slot
            );

            let head_slot_delta = head_state_3.slot.abs_diff(head_state_1.slot);
            assert!(
                head_slot_delta <= 2,
                "Checkpoint-synced node head diverged too much from Node 1. Node 3: {}, Node 1: {}, delta: {head_slot_delta}",
                head_state_3.slot,
                head_state_1.slot,
            );

            let finalized_slot_lag = head_state_1
                .latest_finalized
                .slot
                .saturating_sub(head_state_3.latest_finalized.slot);
            assert!(
                finalized_slot_lag <= 4,
                "Checkpoint-synced node finalized slot lagged too far behind Node 1. Node 3: {}, Node 1: {}, lag: {finalized_slot_lag}",
                head_state_3.latest_finalized.slot,
                head_state_1.latest_finalized.slot,
            );

            if let Some(state_before_restart) = node_3_state_before_restart {
                assert!(
                    head_state_3.slot > state_before_restart.slot,
                    "Restarted checkpoint-sync node did not advance its head. Before restart: {}, after restart: {}",
                    state_before_restart.slot,
                    head_state_3.slot,
                );
                assert!(
                    head_state_3.latest_finalized.slot >= state_before_restart.latest_finalized.slot,
                    "Restarted checkpoint-sync node regressed finalized slot. Before restart: {}, after restart: {}",
                    state_before_restart.latest_finalized.slot,
                    head_state_3.latest_finalized.slot,
                );
            }

            (
                node_executors.into_iter().flatten().collect::<Vec<_>>(),
                retired_node_executors,
            )
        });
        drop(remaining_node_executors);
        drop(retired_node_executors);
    }

    fn create_test_network_config(test_name: &str, num_validators: usize) -> PathBuf {
        let unique_suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("System time is before UNIX epoch")
            .as_nanos();
        let network_config_path = temp_dir().join(format!(
            "{APP_NAME}_{test_name}_{unique_suffix}_network.yaml"
        ));

        let genesis_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("System time is before UNIX epoch")
            .as_secs()
            + 10;

        let validators: String = VALIDATOR_KEYS[..num_validators]
            .iter()
            .map(|(att, prop)| {
                format!("- attestation_public_key: {att}\n  proposal_public_key: {prop}\n")
            })
            .collect();

        let network_yaml = format!(
            "GENESIS_TIME: {genesis_time}\nNUM_VALIDATORS: {num_validators}\nGENESIS_VALIDATORS:\n{validators}"
        );
        fs::write(&network_config_path, network_yaml).expect("Failed to write temp network config");
        network_config_path
    }

    fn create_beacon_test_node_db(test_name: &str, node_index: usize) -> ReamDB {
        let unique_suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("System time is before UNIX epoch")
            .as_nanos();
        let ream_directory = temp_dir().join(format!(
            "{APP_NAME}_{test_name}_{unique_suffix}_node_{node_index}"
        ));

        if ream_directory.exists()
            && let Err(err) = fs::remove_dir_all(&ream_directory)
        {
            warn!("Failed to remove ream directory: {err}");
        }
        fs::create_dir_all(&ream_directory).expect("Failed to create beacon node data directory");
        ReamDB::new(ream_directory).expect("unable to init Ream Database")
    }

    fn beacon_port_offset() -> u16 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("System time is before UNIX epoch")
            .subsec_nanos() as u16
            % 1000
    }

    #[test]
    #[serial]
    fn test_beacon_node_runs_without_panicking() {
        init_test_tracing();

        let config = beacon_node_config_from_args(beacon_port_offset(), None);
        initialize_beacon_e2e_network_spec(config.network.clone());
        let public_keys = beacon_e2e_public_keys();
        let (genesis_state, genesis_block) = build_dev_genesis(&public_keys);

        let db = create_beacon_test_node_db("beacon_node_smoke", 1);
        let genesis_validators_root = seed_beacon_test_db(&db, genesis_state, &genesis_block);
        initialize_beacon_e2e_genesis_root(genesis_validators_root);

        let http_port = config.http_port;
        let control_executor = ReamExecutor::new().unwrap();
        let node_executor = ReamExecutor::new().unwrap();
        let node_executor_handle = node_executor.clone();
        control_executor.clone().runtime().block_on(async move {
            let handle = spawn_beacon_test_node(config, db, node_executor_handle.clone());

            let _identity = wait_for_beacon_identity(http_port).await;

            assert!(
                !handle.is_finished(),
                "beacon node task exited early after identity endpoint became reachable"
            );

            shutdown_beacon_test_node(&node_executor_handle, handle).await;
        });
        node_executor.shutdown_runtime();
    }

    #[test]
    #[serial]
    fn test_beacon_nodes_connect_with_bootnode() {
        init_test_tracing();

        let port_offset = beacon_port_offset();
        let node_1_config = beacon_node_config_from_args(port_offset, None);
        initialize_beacon_e2e_network_spec(node_1_config.network.clone());
        let public_keys = beacon_e2e_public_keys();
        let (genesis_state, genesis_block) = build_dev_genesis(&public_keys);

        let node_1_db = create_beacon_test_node_db("beacon_node_bootnode", 1);
        let node_2_db = create_beacon_test_node_db("beacon_node_bootnode", 2);
        let genesis_validators_root =
            seed_beacon_test_db(&node_1_db, genesis_state.clone(), &genesis_block);
        let node_2_genesis_validators_root =
            seed_beacon_test_db(&node_2_db, genesis_state, &genesis_block);
        assert_eq!(
            genesis_validators_root, node_2_genesis_validators_root,
            "beacon e2e nodes must be seeded from the same genesis"
        );
        initialize_beacon_e2e_genesis_root(genesis_validators_root);

        let control_executor = ReamExecutor::new().unwrap();
        let node_1_http_port = node_1_config.http_port;
        let node_1_executor = ReamExecutor::new().unwrap();
        let node_2_executor = ReamExecutor::new().unwrap();
        let node_1_executor_handle = node_1_executor.clone();
        let node_2_executor_handle = node_2_executor.clone();
        let peer_counts_result = control_executor.clone().runtime().block_on(async move {
            let node_1_handle =
                spawn_beacon_test_node(node_1_config, node_1_db, node_1_executor_handle.clone());

            let node_1_identity = wait_for_beacon_identity(node_1_http_port).await;
            let node_1_enr = node_1_identity["data"]["enr"]
                .as_str()
                .expect("identity response should contain ENR")
                .to_string();

            let node_2_config = beacon_node_config_from_args(port_offset + 1, Some(node_1_enr));
            let node_2_http_port = node_2_config.http_port;
            let node_2_handle =
                spawn_beacon_test_node(node_2_config, node_2_db, node_2_executor_handle.clone());

            let peer_counts =
                wait_for_connected_beacon_peer(&[node_1_http_port, node_2_http_port]).await;
            let node_1_finished = node_1_handle.is_finished();
            let node_2_finished = node_2_handle.is_finished();

            shutdown_beacon_test_node(&node_2_executor_handle, node_2_handle).await;
            shutdown_beacon_test_node(&node_1_executor_handle, node_1_handle).await;
            (peer_counts, node_1_finished, node_2_finished)
        });
        node_2_executor.shutdown_runtime();
        node_1_executor.shutdown_runtime();

        let (peer_counts, node_1_finished, node_2_finished) = peer_counts_result;
        let peer_counts = peer_counts.unwrap_or_else(|peer_counts| {
            panic!("Timed out waiting for beacon nodes to connect: {peer_counts:?}")
        });
        assert!(!node_1_finished, "node 1 task exited early");
        assert!(!node_2_finished, "node 2 task exited early");
        assert!(
            peer_counts
                .iter()
                .any(|count| peer_count_connected(count) > 0),
            "beacon nodes did not report a connected peer: {peer_counts:?}"
        );
    }

    #[test]
    #[serial]
    fn test_beacon_nodes_produce_blocks_and_converge() {
        init_test_tracing();

        let port_offset = beacon_port_offset();
        let mut node_1_config = beacon_node_config_from_args(port_offset, None);
        initialize_beacon_e2e_network_spec(node_1_config.network.clone());
        let public_keys = beacon_e2e_public_keys();
        let (genesis_state, genesis_block) = build_dev_genesis(&public_keys);

        let node_1_db = create_beacon_test_node_db("beacon_node_produce_blocks", 1);
        let node_2_db = create_beacon_test_node_db("beacon_node_produce_blocks", 2);
        let genesis_execution_block_hash = genesis_state.latest_execution_payload_header.block_hash;
        let genesis_validators_root =
            seed_beacon_test_db(&node_1_db, genesis_state.clone(), &genesis_block);
        let node_2_genesis_validators_root =
            seed_beacon_test_db(&node_2_db, genesis_state, &genesis_block);
        assert_eq!(
            genesis_validators_root, node_2_genesis_validators_root,
            "beacon e2e node 2 must be seeded from the same genesis"
        );
        initialize_beacon_e2e_genesis_root(genesis_validators_root);

        let control_executor = ReamExecutor::new().unwrap();
        let node_1_executor = ReamExecutor::new().unwrap();
        let node_2_executor = ReamExecutor::new().unwrap();
        let validator_executors: Vec<_> = (0..BEACON_E2E_VALIDATOR_NODE_COUNT)
            .map(|_| ReamExecutor::new().unwrap())
            .collect();
        let node_1_executor_handle = node_1_executor.clone();
        let node_2_executor_handle = node_2_executor.clone();
        let validator_executor_handles = validator_executors.to_vec();
        let node_1_http_port = node_1_config.http_port;

        let result = control_executor.clone().runtime().block_on(async move {
            let test_dir = temp_dir().join(format!(
                "{APP_NAME}_beacon_produce_blocks_{}",
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .expect("System time is before UNIX epoch")
                    .as_nanos()
            ));
            fs::create_dir_all(&test_dir).expect("Failed to create beacon e2e test dir");
            let jwt_secret_path = test_dir.join("jwt.hex");
            fs::write(
                &jwt_secret_path,
                "0x2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a",
            )
            .expect("Failed to write mock EL JWT secret");

            let mock_execution_server = MockExecutionServer::start(genesis_execution_block_hash);
            let execution_endpoint = mock_execution_server.url();
            node_1_config.execution_endpoint = Some(execution_endpoint.clone());
            node_1_config.execution_jwt_secret = Some(jwt_secret_path.clone());

            let node_1_handle =
                spawn_beacon_test_node(node_1_config, node_1_db, node_1_executor_handle.clone());
            let node_1_identity = wait_for_beacon_identity(node_1_http_port).await;
            let node_1_enr = node_1_identity["data"]["enr"]
                .as_str()
                .expect("identity response should contain ENR")
                .to_string();

            let mut node_2_config =
                beacon_node_config_from_args(port_offset + 1, Some(node_1_enr.clone()));
            node_2_config.execution_endpoint = Some(execution_endpoint);
            node_2_config.execution_jwt_secret = Some(jwt_secret_path);
            let node_2_http_port = node_2_config.http_port;
            let node_2_handle =
                spawn_beacon_test_node(node_2_config, node_2_db, node_2_executor_handle.clone());
            let peer_counts =
                wait_for_connected_beacon_peer(&[node_1_http_port, node_2_http_port]).await;

            let validator_handles = spawn_validator_test_nodes(
                &[node_1_http_port, node_2_http_port],
                &test_dir,
                &validator_executor_handles,
            );

            let finality_target_slot = SLOTS_PER_EPOCH * 2 + 4;
            let first_head = wait_for_head_slot_at_least(node_1_http_port, 4).await;
            let second_head = wait_for_head_slot_at_least(node_1_http_port, first_head.0 + 1).await;
            assert!(
                second_head.0 > first_head.0,
                "beacon node head did not keep advancing: first={first_head:?}, second={second_head:?}"
            );
            let head_at_finality_target =
                wait_for_head_slot_at_least(node_1_http_port, finality_target_slot).await;

            let finality_statuses =
                wait_for_finality_checkpoints_advanced_all(&[node_1_http_port, node_2_http_port])
                    .await;
            let node_1_finality = (
                finality_statuses[0].justified_epoch,
                finality_statuses[0].finalized_epoch,
            );
            let node_2_finality = (
                finality_statuses[1].justified_epoch,
                finality_statuses[1].finalized_epoch,
            );

            // Wait until all beacon nodes converge on the same non-genesis head.
            let matching_head =
                wait_for_matching_heads_all(&[node_1_http_port, node_2_http_port]).await;
            for peer_http_port in [node_2_http_port] {
                let block_on_peer = wait_for_beacon_json(
                    peer_http_port,
                    &format!("/eth/v2/beacon/blocks/{}", matching_head.0),
                )
                .await;
                assert!(
                    block_on_peer["data"]["message"]["slot"].is_string()
                        || block_on_peer["data"]["message"]["slot"].is_u64(),
                    "peer {peer_http_port} did not return imported block by root: {block_on_peer:?}"
                );
            }

            let node_1_finished = node_1_handle.is_finished();
            let node_2_finished = node_2_handle.is_finished();
            let validator_finished = validator_handles
                .iter()
                .map(tokio::task::JoinHandle::is_finished)
                .collect::<Vec<_>>();

            shutdown_validator_test_nodes(&validator_executor_handles, validator_handles).await;
            shutdown_beacon_test_node(&node_2_executor_handle, node_2_handle).await;
            shutdown_beacon_test_node(&node_1_executor_handle, node_1_handle).await;
            mock_execution_server.stop().await;

            (
                peer_counts,
                node_1_finished,
                node_2_finished,
                validator_finished,
                first_head,
                second_head,
                head_at_finality_target,
                matching_head,
                node_1_finality,
                node_2_finality,
            )
        });

        for validator_executor in validator_executors {
            validator_executor.shutdown_runtime();
        }
        node_2_executor.shutdown_runtime();
        node_1_executor.shutdown_runtime();

        let (
            peer_counts,
            node_1_finished,
            node_2_finished,
            validators_finished,
            first_head,
            second_head,
            head_at_finality_target,
            matching_head,
            node_1_finality,
            node_2_finality,
        ) = result;
        let peer_counts = peer_counts.unwrap_or_else(|peer_counts| {
            panic!("Timed out waiting for beacon nodes to connect: {peer_counts:?}")
        });
        assert!(!node_1_finished, "node 1 task exited early");
        assert!(!node_2_finished, "node 2 task exited early");
        assert!(
            validators_finished
                .iter()
                .all(|validator_finished| !validator_finished),
            "validator node task exited early: {validators_finished:?}"
        );
        assert!(
            peer_counts
                .iter()
                .any(|count| peer_count_connected(count) > 0),
            "beacon nodes did not report a connected peer: {peer_counts:?}"
        );
        info!(
            ?first_head,
            ?second_head,
            ?head_at_finality_target,
            ?matching_head,
            ?node_1_finality,
            ?node_2_finality,
            ?validators_finished,
            "Beacon block production e2e test completed"
        );
    }

    #[test]
    #[serial]
    fn test_beacon_nodes_propagate_data_column_sidecars() {
        init_test_tracing();

        let port_offset = beacon_port_offset();
        let mut node_1_config = beacon_node_config_from_args(port_offset, None);
        initialize_beacon_e2e_network_spec(node_1_config.network.clone());
        let public_keys = beacon_e2e_public_keys();
        let (genesis_state, genesis_block) = build_dev_genesis(&public_keys);

        let node_1_db = create_beacon_test_node_db("beacon_node_data_column_sidecars", 1);
        let node_2_db = create_beacon_test_node_db("beacon_node_data_column_sidecars", 2);
        let genesis_execution_block_hash = genesis_state.latest_execution_payload_header.block_hash;
        let genesis_validators_root =
            seed_beacon_test_db(&node_1_db, genesis_state.clone(), &genesis_block);
        let node_2_genesis_validators_root =
            seed_beacon_test_db(&node_2_db, genesis_state, &genesis_block);
        assert_eq!(
            genesis_validators_root, node_2_genesis_validators_root,
            "beacon e2e node 2 must be seeded from the same genesis"
        );
        initialize_beacon_e2e_genesis_root(genesis_validators_root);

        // Cheap clones, kept for inspecting storage while the node tasks are still running.
        let node_1_db_for_check = node_1_db.clone();
        let node_2_db_for_check = node_2_db.clone();

        let control_executor = ReamExecutor::new().unwrap();
        let node_1_executor = ReamExecutor::new().unwrap();
        let node_2_executor = ReamExecutor::new().unwrap();
        let validator_executors: Vec<_> = (0..BEACON_E2E_VALIDATOR_NODE_COUNT)
            .map(|_| ReamExecutor::new().unwrap())
            .collect();
        let node_1_executor_handle = node_1_executor.clone();
        let node_2_executor_handle = node_2_executor.clone();
        let validator_executor_handles = validator_executors.to_vec();
        let node_1_http_port = node_1_config.http_port;

        let result = control_executor.clone().runtime().block_on(async move {
            let test_dir = temp_dir().join(format!(
                "{APP_NAME}_beacon_data_column_sidecars_{}",
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .expect("System time is before UNIX epoch")
                    .as_nanos()
            ));
            fs::create_dir_all(&test_dir).expect("Failed to create beacon e2e test dir");
            let jwt_secret_path = test_dir.join("jwt.hex");
            fs::write(
                &jwt_secret_path,
                "0x2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a",
            )
            .expect("Failed to write mock EL JWT secret");

            let mock_execution_server = MockExecutionServer::start(genesis_execution_block_hash);
            // Every payload produced from here on carries one blob.
            mock_execution_server
                .generator()
                .lock()
                .expect("mock EL generator lock poisoned")
                .set_blobs_per_payload(1);
            let execution_endpoint = mock_execution_server.url();
            node_1_config.execution_endpoint = Some(execution_endpoint.clone());
            node_1_config.execution_jwt_secret = Some(jwt_secret_path.clone());

            let node_1_handle =
                spawn_beacon_test_node(node_1_config, node_1_db, node_1_executor_handle.clone());
            let node_1_identity = wait_for_beacon_identity(node_1_http_port).await;
            let node_1_enr = node_1_identity["data"]["enr"]
                .as_str()
                .expect("identity response should contain ENR")
                .to_string();

            let mut node_2_config =
                beacon_node_config_from_args(port_offset + 1, Some(node_1_enr.clone()));
            node_2_config.execution_endpoint = Some(execution_endpoint);
            node_2_config.execution_jwt_secret = Some(jwt_secret_path);
            let node_2_http_port = node_2_config.http_port;
            let node_2_handle =
                spawn_beacon_test_node(node_2_config, node_2_db, node_2_executor_handle.clone());
            let peer_counts =
                wait_for_connected_beacon_peer(&[node_1_http_port, node_2_http_port]).await;

            let validator_handles = spawn_validator_test_nodes(
                &[node_1_http_port, node_2_http_port],
                &test_dir,
                &validator_executor_handles,
            );

            // Slots can be missed, so wait a few out then scan for one that has blobs.
            let warmup_head = wait_for_head_slot_at_least(node_1_http_port, 3).await;

            // One block with blobs is enough; stop so later slots don't add to the (real, and in
            // debug builds slow) KZG workload while we wait for propagation below.
            mock_execution_server
                .generator()
                .lock()
                .expect("mock EL generator lock poisoned")
                .set_blobs_per_payload(0);

            let (target_slot, target_block) = find_slot_with_blob_commitments(
                node_1_http_port,
                warmup_head.0,
            )
            .await
            .unwrap_or_else(|| {
                panic!(
                    "no block up to slot {} carried blob commitments while blobs were enabled",
                    warmup_head.0
                )
            });
            let commitment_count = target_block["data"]["message"]["body"]["blob_kzg_commitments"]
                .as_array()
                .map(Vec::len)
                .unwrap_or(0);

            let root_response = wait_for_beacon_json(
                node_1_http_port,
                &format!("/eth/v1/beacon/blocks/{target_slot}/root"),
            )
            .await;
            let block_root: B256 = root_response["data"]["root"]
                .as_str()
                .expect("root response should include a root")
                .parse()
                .expect("root should be a valid B256");

            // Make sure node_2 has imported the same block before checking its column storage.
            wait_for_head_slot_at_least(node_2_http_port, target_slot).await;

            // `on_block` no longer waits on availability, so head can advance before all 128
            // columns have propagated - wait for them explicitly on both nodes.
            wait_for_all_data_column_sidecars(&node_1_db_for_check, block_root).await;
            wait_for_all_data_column_sidecars(&node_2_db_for_check, block_root).await;

            let node_1_finished = node_1_handle.is_finished();
            let node_2_finished = node_2_handle.is_finished();
            let validator_finished = validator_handles
                .iter()
                .map(tokio::task::JoinHandle::is_finished)
                .collect::<Vec<_>>();

            shutdown_validator_test_nodes(&validator_executor_handles, validator_handles).await;
            shutdown_beacon_test_node(&node_2_executor_handle, node_2_handle).await;
            shutdown_beacon_test_node(&node_1_executor_handle, node_1_handle).await;
            mock_execution_server.stop().await;

            (
                peer_counts,
                node_1_finished,
                node_2_finished,
                validator_finished,
                target_slot,
                block_root,
                commitment_count,
            )
        });

        for validator_executor in validator_executors {
            validator_executor.shutdown_runtime();
        }
        node_2_executor.shutdown_runtime();
        node_1_executor.shutdown_runtime();

        let (
            peer_counts,
            node_1_finished,
            node_2_finished,
            validators_finished,
            target_slot,
            block_root,
            commitment_count,
        ) = result;
        let peer_counts = peer_counts.unwrap_or_else(|peer_counts| {
            panic!("Timed out waiting for beacon nodes to connect: {peer_counts:?}")
        });
        assert!(!node_1_finished, "node 1 task exited early");
        assert!(!node_2_finished, "node 2 task exited early");
        assert!(
            validators_finished
                .iter()
                .all(|validator_finished| !validator_finished),
            "validator node task exited early: {validators_finished:?}"
        );
        assert!(
            peer_counts
                .iter()
                .any(|count| peer_count_connected(count) > 0),
            "beacon nodes did not report a connected peer: {peer_counts:?}"
        );

        info!(
            target_slot,
            ?block_root,
            commitment_count,
            ?validators_finished,
            "Beacon data column sidecar propagation e2e test completed"
        );
    }

    #[test]
    #[serial]
    fn test_beacon_nodes_propagate_data_column_sidecars_multi_hop() {
        init_test_tracing();

        let port_offset = beacon_port_offset();
        let mut node_1_config = beacon_node_config_from_args(port_offset, None);
        // Both chain endpoints have active discovery disabled so the only way node_3 can end up
        // with node_1's sidecars is via node_2 forwarding them - not by discovering node_1
        // directly.
        node_1_config.disable_discovery = true;
        initialize_beacon_e2e_network_spec(node_1_config.network.clone());
        let public_keys = beacon_e2e_public_keys();
        let (genesis_state, genesis_block) = build_dev_genesis(&public_keys);

        let node_1_db = create_beacon_test_node_db("beacon_node_data_column_sidecars_multi_hop", 1);
        let node_2_db = create_beacon_test_node_db("beacon_node_data_column_sidecars_multi_hop", 2);
        let node_3_db = create_beacon_test_node_db("beacon_node_data_column_sidecars_multi_hop", 3);
        let genesis_execution_block_hash = genesis_state.latest_execution_payload_header.block_hash;
        let genesis_validators_root =
            seed_beacon_test_db(&node_1_db, genesis_state.clone(), &genesis_block);
        let node_2_genesis_validators_root =
            seed_beacon_test_db(&node_2_db, genesis_state.clone(), &genesis_block);
        let node_3_genesis_validators_root =
            seed_beacon_test_db(&node_3_db, genesis_state, &genesis_block);
        assert_eq!(
            genesis_validators_root, node_2_genesis_validators_root,
            "beacon e2e node 2 must be seeded from the same genesis"
        );
        assert_eq!(
            genesis_validators_root, node_3_genesis_validators_root,
            "beacon e2e node 3 must be seeded from the same genesis"
        );
        initialize_beacon_e2e_genesis_root(genesis_validators_root);

        // Cheap clones, kept for inspecting storage while the node tasks are still running.
        let node_1_db_for_check = node_1_db.clone();
        let node_2_db_for_check = node_2_db.clone();
        let node_3_db_for_check = node_3_db.clone();

        let control_executor = ReamExecutor::new().unwrap();
        let node_1_executor = ReamExecutor::new().unwrap();
        let node_2_executor = ReamExecutor::new().unwrap();
        let node_3_executor = ReamExecutor::new().unwrap();
        let validator_executors: Vec<_> = (0..BEACON_E2E_VALIDATOR_NODE_COUNT)
            .map(|_| ReamExecutor::new().unwrap())
            .collect();
        let node_1_executor_handle = node_1_executor.clone();
        let node_2_executor_handle = node_2_executor.clone();
        let node_3_executor_handle = node_3_executor.clone();
        let validator_executor_handles = validator_executors.to_vec();
        let node_1_http_port = node_1_config.http_port;

        let result = control_executor.clone().runtime().block_on(async move {
            let test_dir = temp_dir().join(format!(
                "{APP_NAME}_beacon_data_column_sidecars_multi_hop_{}",
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .expect("System time is before UNIX epoch")
                    .as_nanos()
            ));
            fs::create_dir_all(&test_dir).expect("Failed to create beacon e2e test dir");
            let jwt_secret_path = test_dir.join("jwt.hex");
            fs::write(
                &jwt_secret_path,
                "0x2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a2a",
            )
            .expect("Failed to write mock EL JWT secret");

            let mock_execution_server = MockExecutionServer::start(genesis_execution_block_hash);
            // Every payload produced from here on carries one blob.
            mock_execution_server
                .generator()
                .lock()
                .expect("mock EL generator lock poisoned")
                .set_blobs_per_payload(1);
            let execution_endpoint = mock_execution_server.url();
            node_1_config.execution_endpoint = Some(execution_endpoint.clone());
            node_1_config.execution_jwt_secret = Some(jwt_secret_path.clone());

            let node_1_handle =
                spawn_beacon_test_node(node_1_config, node_1_db, node_1_executor_handle.clone());
            let node_1_identity = wait_for_beacon_identity(node_1_http_port).await;
            let node_1_enr = node_1_identity["data"]["enr"]
                .as_str()
                .expect("identity response should contain ENR")
                .to_string();

            let mut node_2_config =
                beacon_node_config_from_args(port_offset + 1, Some(node_1_enr.clone()));
            node_2_config.execution_endpoint = Some(execution_endpoint.clone());
            node_2_config.execution_jwt_secret = Some(jwt_secret_path.clone());
            let node_2_http_port = node_2_config.http_port;
            let node_2_handle =
                spawn_beacon_test_node(node_2_config, node_2_db, node_2_executor_handle.clone());
            let node_2_identity = wait_for_beacon_identity(node_2_http_port).await;
            let node_2_enr = node_2_identity["data"]["enr"]
                .as_str()
                .expect("identity response should contain ENR")
                .to_string();

            // node_3 is only ever told about node_2's ENR, never node_1's, and can't discover
            // node_1 through node_2's routing table either since its own discovery is disabled.
            let mut node_3_config = beacon_node_config_from_args(port_offset + 2, Some(node_2_enr));
            node_3_config.disable_discovery = true;
            node_3_config.execution_endpoint = Some(execution_endpoint);
            node_3_config.execution_jwt_secret = Some(jwt_secret_path);
            let node_3_http_port = node_3_config.http_port;
            let node_3_handle =
                spawn_beacon_test_node(node_3_config, node_3_db, node_3_executor_handle.clone());

            let peer_counts = wait_for_connected_beacon_peer(&[
                node_1_http_port,
                node_2_http_port,
                node_3_http_port,
            ])
            .await;

            let validator_handles = spawn_validator_test_nodes(
                &[node_1_http_port, node_2_http_port],
                &test_dir,
                &validator_executor_handles,
            );

            // Slots can be missed, so wait a few out then scan for one that has blobs.
            let warmup_head = wait_for_head_slot_at_least(node_1_http_port, 3).await;

            // One block with blobs is enough; stop so later slots don't add to the (real, and in
            // debug builds slow) KZG workload while we wait for propagation below.
            mock_execution_server
                .generator()
                .lock()
                .expect("mock EL generator lock poisoned")
                .set_blobs_per_payload(0);

            let (target_slot, target_block) = find_slot_with_blob_commitments(
                node_1_http_port,
                warmup_head.0,
            )
            .await
            .unwrap_or_else(|| {
                panic!(
                    "no block up to slot {} carried blob commitments while blobs were enabled",
                    warmup_head.0
                )
            });
            let commitment_count = target_block["data"]["message"]["body"]["blob_kzg_commitments"]
                .as_array()
                .map(Vec::len)
                .unwrap_or(0);

            let root_response = wait_for_beacon_json(
                node_1_http_port,
                &format!("/eth/v1/beacon/blocks/{target_slot}/root"),
            )
            .await;
            let block_root: B256 = root_response["data"]["root"]
                .as_str()
                .expect("root response should include a root")
                .parse()
                .expect("root should be a valid B256");

            // node_3 has no direct link to node_1 (the proposer), so it reaching the same head
            // and storing every column proves node_2 actually forwarded the gossip onward.
            wait_for_head_slot_at_least(node_2_http_port, target_slot).await;
            wait_for_head_slot_at_least(node_3_http_port, target_slot).await;

            wait_for_all_data_column_sidecars(&node_1_db_for_check, block_root).await;
            wait_for_all_data_column_sidecars(&node_2_db_for_check, block_root).await;
            wait_for_all_data_column_sidecars(&node_3_db_for_check, block_root).await;

            let node_1_finished = node_1_handle.is_finished();
            let node_2_finished = node_2_handle.is_finished();
            let node_3_finished = node_3_handle.is_finished();
            let validator_finished = validator_handles
                .iter()
                .map(tokio::task::JoinHandle::is_finished)
                .collect::<Vec<_>>();

            shutdown_validator_test_nodes(&validator_executor_handles, validator_handles).await;
            shutdown_beacon_test_node(&node_3_executor_handle, node_3_handle).await;
            shutdown_beacon_test_node(&node_2_executor_handle, node_2_handle).await;
            shutdown_beacon_test_node(&node_1_executor_handle, node_1_handle).await;
            mock_execution_server.stop().await;

            (
                peer_counts,
                node_1_finished,
                node_2_finished,
                node_3_finished,
                validator_finished,
                target_slot,
                block_root,
                commitment_count,
            )
        });

        for validator_executor in validator_executors {
            validator_executor.shutdown_runtime();
        }
        node_3_executor.shutdown_runtime();
        node_2_executor.shutdown_runtime();
        node_1_executor.shutdown_runtime();

        let (
            peer_counts,
            node_1_finished,
            node_2_finished,
            node_3_finished,
            validators_finished,
            target_slot,
            block_root,
            commitment_count,
        ) = result;
        let peer_counts = peer_counts.unwrap_or_else(|peer_counts| {
            panic!("Timed out waiting for beacon nodes to connect: {peer_counts:?}")
        });
        assert!(!node_1_finished, "node 1 task exited early");
        assert!(!node_2_finished, "node 2 task exited early");
        assert!(!node_3_finished, "node 3 task exited early");
        assert!(
            validators_finished
                .iter()
                .all(|validator_finished| !validator_finished),
            "validator node task exited early: {validators_finished:?}"
        );
        assert!(
            peer_counts
                .iter()
                .any(|count| peer_count_connected(count) > 0),
            "beacon nodes did not report a connected peer: {peer_counts:?}"
        );

        info!(
            target_slot,
            ?block_root,
            commitment_count,
            ?validators_finished,
            "Beacon data column sidecar multi-hop propagation e2e test completed"
        );
    }

    #[test]
    #[serial]
    fn test_lean_node_runs_10_seconds_without_panicking() {
        let cli = Cli::parse_from([
            "ream",
            "--ephemeral",
            "lean_node",
            "--network",
            "ephemery",
            "--validator-registry-path",
            "./assets/lean/annotated_validators.yaml",
            "--is-aggregator",
        ]);

        let Commands::LeanNode(config) = cli.command else {
            panic!("Expected lean_node command");
        };

        let ream_directory = setup_data_dir(APP_NAME, None, true).unwrap();
        let db = ReamDB::new(ream_directory).unwrap();
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

    #[test]
    #[serial]
    fn test_lean_node_finalizes() {
        if let Err(err) = tracing_subscriber::fmt()
            .with_env_filter(Verbosity::Info.directive())
            .with_test_writer()
            .try_init()
        {
            warn!("Failed to initialize tracing subscriber: {err}");
        }

        let cli = Cli::parse_from([
            "ream",
            "--ephemeral",
            "lean_node",
            "--network",
            "ephemery",
            "--validator-registry-path",
            "./assets/lean/annotated_validators.yaml",
            "--socket-port",
            "9001",
            "--http-port",
            "5053",
            "--is-aggregator",
        ]);

        let Commands::LeanNode(config) = cli.command else {
            panic!("Expected lean_node command");
        };

        let ream_directory = setup_data_dir(APP_NAME, None, true).unwrap();
        let db = ReamDB::new(ream_directory).unwrap();
        let executor = ReamExecutor::new().unwrap();
        let executor_handle = executor.clone();

        let cloned_db = db.clone();
        executor.runtime().block_on(async move {
            let handle = tokio::spawn(async move {
                run_lean_node(*config, executor_handle, cloned_db).await;
            });

            let result = timeout(Duration::from_secs(120), async {
                sleep(Duration::from_secs(120)).await;
                Ok::<_, ()>(())
            })
            .await;

            match result {
                Ok(Ok(())) => {}
                Err(err) => panic!("lean_node panicked or exited early {err:?}"),
                Ok(Err(err)) => panic!("internal error {err:?}"),
            }

            handle.abort();

            sleep(Duration::from_secs(2)).await;
        });

        let lean_db = db.init_lean_db().unwrap();
        let head = lean_db.head_provider().get().unwrap();
        let head_state = lean_db.state_provider().get(head).unwrap().unwrap();

        let justfication_lag = 2;
        let finalization_lag = 2;

        info!(
            "Test results: head_slot={}, justified_slot={}, finalized_slot={}, head_root={:?}",
            head_state.slot,
            head_state.latest_justified.slot,
            head_state.latest_finalized.slot,
            head
        );

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
            head_state.latest_justified.slot + justfication_lag <= head_state.slot,
            "Expected the head to be at least {justfication_lag} slots ahead of the justified checkpoint {:?} + {justfication_lag} vs {:?}",
            head_state.latest_justified.slot,
            head_state.slot
        );
        assert!(
            head_state.latest_finalized.slot + finalization_lag <= head_state.slot,
            "Expected the head to be at least {finalization_lag} slots ahead of the finalized checkpoint {:?} + {finalization_lag} vs {:?}",
            head_state.latest_finalized.slot,
            head_state.slot
        );
    }

    fn generate_node_identity(path: &PathBuf) -> String {
        let secp256k1_key = secp256k1::Keypair::generate();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("Failed to create key directory");
        }
        fs::write(path, hex::encode(secp256k1_key.secret().to_bytes()))
            .expect("Failed to write private key");
        let id_keypair = Keypair::from(secp256k1_key);
        id_keypair.public().to_peer_id().to_string()
    }

    #[test]
    #[serial]
    #[ignore = "I am not sure if this topology is supposed to work or not"]
    fn test_lean_node_finalizes_linear_1_2_1() {
        let topology = vec![vec![], vec![0], vec![1]];
        run_multi_node_finalization_test(topology, "linear_1_2_1");
    }

    #[test]
    #[serial]
    fn test_lean_node_finalizes_mesh_2_2_2() {
        let topology = vec![vec![], vec![0], vec![0, 1]];
        run_multi_node_finalization_test(topology, "mesh_2_2_2");
    }

    fn run_multi_node_finalization_test(topology: Vec<Vec<usize>>, test_name: &str) {
        if let Err(err) = tracing_subscriber::fmt()
            .with_env_filter(Verbosity::Info.directive())
            .with_test_writer()
            .try_init()
        {
            warn!("Failed to initialize tracing subscriber: {err}");
        }

        info!("Starting multi-node finalization test: {}", test_name);

        let test_duration_secs = 70;
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

        let registry_path = write_test_validator_registry(&assets_directory, test_name, node_count);
        let registry_path_string = registry_path.to_string_lossy().to_string();

        let network_config_path = create_test_network_config(test_name, 3);
        let network_config_path_string = network_config_path.to_string_lossy().to_string();

        let executor = ReamExecutor::new().unwrap();
        executor.clone().runtime().block_on(async move {
            let mut node_handles = Vec::new();
            let mut node_addresses: Vec<String> = Vec::new();
            let mut db_instances = Vec::new();

            for (i, node_boot_config) in topology.iter().enumerate() {
                let node_index = i + 1;
                let node_id = format!("node{node_index}");

                let ream_directory =
                    temp_dir().join(format!("{APP_NAME}_{test_name}_node_{node_index}"));

                if ream_directory.exists()
                    && let Err(err) = fs::remove_dir_all(&ream_directory)
                {
                    warn!("Failed to remove ream directory: {err}");
                }
                fs::create_dir_all(&ream_directory).expect("Failed to create data directory");

                let key_path = ream_directory.join("node_key");
                let peer_id = generate_node_identity(&key_path);

                let port_offset = (test_name.len() as u16) % 100;
                let p2p_port = base_p2p_port + port_offset + (i as u16);
                let http_port = base_http_port + port_offset + (i as u16);

                let address = format!("/ip4/127.0.0.1/udp/{p2p_port}/quic-v1/p2p/{peer_id}");
                node_addresses.push(address.clone());

                if i == 0 {
                    info!("BOOTNODE ADDRESS: {address}");
                }

                db_instances.push(ReamDB::new(ream_directory.clone()).unwrap());

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
                    network_config_path_string.clone(),
                    "--validator-registry-path".to_string(),
                    registry_path_string.clone(),
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

                if i == 0 {
                    arguments.push("--is-aggregator".to_string());
                }

                if !bootnode_arguments.is_empty() {
                    arguments.push("--bootnodes".to_string());
                    arguments.push(bootnode_arguments.join(","));
                }

                let cli = Cli::parse_from(arguments);
                let Commands::LeanNode(config) = cli.command else {
                    panic!("Expected lean_node command");
                };

                let node_executor = executor.clone();
                let db = db_instances[i].clone();
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
                let start = Instant::now();
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

            if let Err(err) = fs::remove_file(&registry_path) {
                warn!("Failed to remove registry file: {err}");
            }
            if let Err(err) = fs::remove_file(&network_config_path) {
                warn!("Failed to remove network config file: {err}");
            }
            for handle in node_handles {
                handle.abort();
            }

            sleep(Duration::from_secs(2)).await;

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
    #[serial]
    fn test_lean_node_syncs_and_finalizes_late_joiner() {
        // Topology: Node 3 connects to Node 1 and Node 2.
        // Node 1 and 2 start immediately. Node 3 starts after 50s.
        let topology = [vec![], vec![0], vec![0, 1]];
        let test_name = "late_joiner_sync";

        if let Err(err) = tracing_subscriber::fmt()
            .with_env_filter(Verbosity::Info.directive())
            .with_test_writer()
            .try_init()
        {
            warn!("Failed to initialize tracing subscriber: {err}");
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

        let registry_path = write_test_validator_registry(&assets_directory, test_name, node_count);
        let registry_path_string = registry_path.to_string_lossy().to_string();

        let network_config_path = create_test_network_config(test_name, 3);
        let network_config_path_string = network_config_path.to_string_lossy().to_string();

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

                let ream_directory =
                    temp_dir().join(format!("{APP_NAME}_{test_name}_node_{node_index}"));

                if ream_directory.exists()
                    && let Err(err) = fs::remove_dir_all(&ream_directory)
                {
                    warn!("Failed to remove ream directory: {err}");
                }
                fs::create_dir_all(&ream_directory).expect("Failed to create data directory");

                let key_path = ream_directory.join("node_key");
                let peer_id = generate_node_identity(&key_path);

                let port_offset = (test_name.len() as u16) % 100;
                let p2p_port = base_p2p_port + port_offset + (i as u16);

                let address = format!("/ip4/127.0.0.1/udp/{p2p_port}/quic-v1/p2p/{peer_id}");
                node_addresses.push(address.clone());

                db_instances[i] = Some(ReamDB::new(ream_directory.clone()).unwrap());
            }

            for (i, node_boot_config) in topology.iter().enumerate() {
                let node_index = i + 1;
                let db = db_instances[i].clone().unwrap();
                let key_path = temp_dir()
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
                    network_config_path_string.clone(),
                    "--validator-registry-path".to_string(),
                    registry_path_string.clone(),
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

                if i == 0 {
                    arguments.push("--is-aggregator".to_string());
                }

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

            let start_time = Instant::now();
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

            if let Err(err) = fs::remove_file(&registry_path) {
                warn!("Failed to remove registry file: {err}");
            }
            if let Err(err) = fs::remove_file(&network_config_path) {
                warn!("Failed to remove network config file: {err}");
            }
            for handle in node_handles.into_iter().flatten() {
                handle.abort();
            }

            sleep(Duration::from_secs(2)).await;

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

            let head_slot_delta = head_state.slot.abs_diff(head_state_1.slot);
            let finalized_slot_lag = head_state_1
                .latest_finalized
                .slot
                .saturating_sub(head_state.latest_finalized.slot);
            let node_3_head_slot = head_state.slot;
            let node_1_head_slot = head_state_1.slot;
            let node_3_finalized_slot = head_state.latest_finalized.slot;
            let node_1_finalized_slot = head_state_1.latest_finalized.slot;
            assert!(
                head_slot_delta <= 2,
                "Node 3 head diverged too much from Node 1. Node 3: {node_3_head_slot}, Node 1: {node_1_head_slot}, delta: {head_slot_delta}"
            );
            assert!(
                finalized_slot_lag <= 4,
                "Node 3 finalized slot lagged too far behind Node 1. Node 3: {node_3_finalized_slot}, Node 1: {node_1_finalized_slot}, lag: {finalized_slot_lag}"
            );
        });
    }

    #[test]
    #[serial]
    fn test_lean_node_checkpoint_sync_from_running_node() {
        let test_duration_secs = 240;

        run_checkpoint_sync_scenario(CheckpointSyncScenario {
            test_name: "checkpoint_sync_late_joiner",
            test_duration_secs,
            checkpoint_sync_start_delay: 100,
            restart_delay_after_node_3_start: None,
            preseed_node_3_before_checkpoint_sync: false,
        });
    }

    #[test]
    #[serial]
    fn test_lean_node_checkpoint_sync_from_fresh_source() {
        run_checkpoint_sync_scenario(CheckpointSyncScenario {
            test_name: "checkpoint_sync_fresh_source",
            test_duration_secs: 140,
            checkpoint_sync_start_delay: 0,
            restart_delay_after_node_3_start: None,
            preseed_node_3_before_checkpoint_sync: false,
        });
    }

    #[test]
    #[serial]
    fn test_lean_node_checkpoint_sync_restart_existing_node() {
        run_checkpoint_sync_scenario(CheckpointSyncScenario {
            test_name: "checkpoint_sync_restart_existing_node",
            test_duration_secs: 170,
            checkpoint_sync_start_delay: 70,
            restart_delay_after_node_3_start: Some(35),
            preseed_node_3_before_checkpoint_sync: false,
        });
    }

    #[test]
    #[serial]
    fn test_lean_node_checkpoint_sync_from_existing_data_dir() {
        run_checkpoint_sync_scenario(CheckpointSyncScenario {
            test_name: "checkpoint_sync_existing_data_dir",
            test_duration_secs: 170,
            checkpoint_sync_start_delay: 45,
            restart_delay_after_node_3_start: None,
            preseed_node_3_before_checkpoint_sync: true,
        });
    }

    #[test]
    #[serial]
    fn test_lean_node_syncs_and_finalizes_two_nodes() {
        if std::env::var("REAM_RUN_INTEROP_TESTS").unwrap_or_default() != "1" {
            info!("Skipping interop test: set REAM_RUN_INTEROP_TESTS=1 to enable");
            return;
        }

        let known_good_bin = std::env::var("REAM_KNOWN_GOOD_BIN")
            .expect("Missing REAM_KNOWN_GOOD_BIN: set path to known-good `ream` binary");
        assert!(
            PathBuf::from(&known_good_bin).exists(),
            "REAM_KNOWN_GOOD_BIN path does not exist: {known_good_bin}"
        );

        if let Err(err) = tracing_subscriber::fmt()
            .with_env_filter(Verbosity::Info.directive())
            .with_test_writer()
            .try_init()
        {
            warn!("Failed to initialize tracing subscriber: {err}");
        }

        let topology = [vec![], vec![0]];
        let test_name = "two_nodes_sync_from_genesis";

        let test_duration_secs = 90;
        let base_p2p_port = 22600;
        let base_http_port = 18652;
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

        let registry_path = write_test_validator_registry(&assets_directory, test_name, node_count);
        let registry_path_string = registry_path.to_string_lossy().to_string();

        let network_config_path = create_test_network_config(test_name, 2);
        let network_config_path_string = network_config_path.to_string_lossy().to_string();

        let mut node_addresses = Vec::with_capacity(node_count);
        let mut node_data_directories = Vec::with_capacity(node_count);

        for (i, _) in topology.iter().enumerate() {
            let node_index = i + 1;

            let ream_data_directory =
                temp_dir().join(format!("{APP_NAME}_{test_name}_node_{node_index}"));

            if ream_data_directory.exists()
                && let Err(err) = fs::remove_dir_all(&ream_data_directory)
            {
                warn!("Failed to remove ream data directory: {err}");
            }
            fs::create_dir_all(&ream_data_directory).expect("Failed to create data directory");

            let key_path = ream_data_directory.join("node_key");
            let peer_id = generate_node_identity(&key_path);

            let port_offset = (test_name.len() as u16) % 100;
            let p2p_port = base_p2p_port + port_offset + (i as u16);

            let address = format!("/ip4/127.0.0.1/udp/{p2p_port}/quic-v1/p2p/{peer_id}");
            node_addresses.push(address);

            node_data_directories.push(ream_data_directory);
        }

        let mut node_2_configuration_and_database = None;
        let mut process_arguments = Vec::with_capacity(node_count);

        for (i, node_boot_config) in topology.iter().enumerate() {
            let node_index = i + 1;
            let key_path = node_data_directories[i].join("node_key");

            let port_offset = (test_name.len() as u16) % 100;
            let p2p_port = base_p2p_port + port_offset + (i as u16);
            let http_port = base_http_port + port_offset + (i as u16);

            let mut bootnode_arguments = Vec::new();
            for &target_idx in node_boot_config {
                if target_idx < node_addresses.len() {
                    bootnode_arguments.push(node_addresses[target_idx].clone());
                }
            }

            let mut cli_arguments = vec![
                "ream".to_string(),
                "lean_node".to_string(),
                "--network".to_string(),
                network_config_path_string.clone(),
                "--validator-registry-path".to_string(),
                registry_path_string.clone(),
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

            if i == 1 {
                cli_arguments.push("--is-aggregator".to_string());
            }

            if !bootnode_arguments.is_empty() {
                cli_arguments.push("--bootnodes".to_string());
                cli_arguments.push(bootnode_arguments.join(","));
            }

            let cli = Cli::parse_from(cli_arguments.clone());
            let Commands::LeanNode(config) = cli.command else {
                panic!("Expected lean_node command");
            };
            if i == 1 {
                let ream_database = ReamDB::new(node_data_directories[i].clone()).unwrap();
                node_2_configuration_and_database = Some((*config, ream_database));
            }

            let mut node_process_args = vec![
                "--data-dir".to_string(),
                node_data_directories[i].to_string_lossy().to_string(),
            ];
            node_process_args.extend(cli_arguments.into_iter().skip(1));
            process_arguments.push(node_process_args);
        }

        let (node_2_configuration, node_2_ream_database) =
            node_2_configuration_and_database.expect("Missing node 2 configuration");

        let executor = ReamExecutor::new().unwrap();
        executor.clone().runtime().block_on(async move {
            info!("Starting Node 1 from known-good binary: {known_good_bin}");
            let mut known_good_child = Command::new(&known_good_bin)
                .args(&process_arguments[0])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .expect("Failed to start known-good node process");

            use tracing::{Instrument, info_span};

            info!("Starting Node 2 from current branch code...");
            let node_2_executor = executor.clone();
            let node_2_ream_database_for_task = node_2_ream_database.clone();
            let node_2_span = info_span!(
                "lean_node",
                node_id = %node_2_configuration.node_id
            );
            let node_2_handle = tokio::spawn(
                async move {
                    run_lean_node(
                        node_2_configuration,
                        node_2_executor,
                        node_2_ream_database_for_task,
                    )
                    .await;
                }
                .instrument(node_2_span),
            );

            let start_time = Instant::now();

            loop {
                let elapsed = start_time.elapsed().as_secs();
                if elapsed >= test_duration_secs {
                    break;
                }

                if let Some(status) = known_good_child
                    .try_wait()
                    .expect("Failed to poll known-good node process")
                {
                    panic!("Known-good node exited early with status: {status}");
                }

                sleep(Duration::from_secs(2)).await;

                if let Ok(lean_database) = node_2_ream_database.init_lean_db()
                    && let Ok(head) = lean_database.head_provider().get()
                    && let Ok(Some(state)) = lean_database.state_provider().get(head)
                {
                    info!(
                        "Node 2 Chain: Slot={} | Finalized={}",
                        state.slot,
                        state.latest_finalized.slot
                    );
                }
            }

            if let Err(err) = fs::remove_file(&registry_path) {
                warn!("Failed to remove registry file: {err}");
            }
            if let Err(err) = fs::remove_file(&network_config_path) {
                warn!("Failed to remove network config file: {err}");
            }
            node_2_handle.abort();

            let _ = known_good_child.kill();
            let _ = known_good_child.wait();

            sleep(Duration::from_secs(2)).await;

            let node_1_database = ReamDB::new(node_data_directories[0].clone())
                .unwrap()
                .init_lean_db()
                .unwrap();
            let node_1_state = node_1_database
                .state_provider()
                .get(node_1_database.head_provider().get().unwrap())
                .unwrap()
                .unwrap();

            let node_2_database = node_2_ream_database.init_lean_db().unwrap();
            let node_2_state = node_2_database
                .state_provider()
                .get(node_2_database.head_provider().get().unwrap())
                .unwrap()
                .unwrap();

            info!(
                "FINAL: Node 1 Slot: {}, Finalized: {} | Node 2 Slot: {}, Finalized: {}",
                node_1_state.slot,
                node_1_state.latest_finalized.slot,
                node_2_state.slot,
                node_2_state.latest_finalized.slot
            );

            assert!(
                node_1_state.latest_finalized.slot > 0,
                "Known-good node failed to finalize"
            );
            assert!(
                node_2_state.latest_finalized.slot > 0,
                "Current-branch node failed to finalize after syncing"
            );

            let slot_tolerance = 2;
            assert!(
                node_2_state.slot + slot_tolerance >= node_1_state.slot,
                "Current-branch node is too far behind known-good node. Current: {current_slot}, known-good: {known_good_slot}, tolerance: {slot_tolerance}",
                current_slot = node_2_state.slot,
                known_good_slot = node_1_state.slot,
            );
        });
    }
}
