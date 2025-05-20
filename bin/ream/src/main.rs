use std::env;

use clap::Parser;
use ream::cli::{Cli, Commands};
use ream_checkpoint_sync::initialize_db_from_checkpoint;
use ream_consensus::constants::{genesis_validators_root, set_genesis_validator_root};
use ream_discv5::{config::DiscoveryConfig, subnet::Subnets};
use ream_executor::ReamExecutor;
use ream_network_spec::networks::{network_spec, set_network_spec};
use ream_p2p::{
    config::NetworkConfig,
    gossipsub::{
        configurations::GossipsubConfig,
        topics::{GossipTopic, GossipTopicKind},
    },
    network::Network,
};
use ream_rpc::{config::ServerConfig, start_server};
use ream_storage::{
    db::{ReamDB, reset_db},
    dir::setup_data_dir,
};
use tracing::{error, info};
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
        Commands::Node(config) => {
            info!("starting up...");

            set_network_spec(config.network);

            let server_config = ServerConfig::new(
                config.http_address,
                config.http_port,
                config.http_allow_origin,
            );

            let discv5_config = discv5::ConfigBuilder::new(discv5::ListenConfig::from_ip(
                config.socket_address,
                config.discovery_port,
            ))
            .build();

            let bootnodes = config.bootnodes.to_enrs(network_spec().network.clone());
            let discv5_config = DiscoveryConfig {
                discv5_config,
                bootnodes,
                socket_address: config.socket_address,
                socket_port: config.socket_port,
                discovery_port: config.discovery_port,
                disable_discovery: config.disable_discovery,
                subnets: Subnets::new(),
            };

            let ream_dir = setup_data_dir(APP_NAME, config.data_dir.clone(), config.ephemeral)
                .expect("Unable to initialize database directory");

            if config.purge_db {
                reset_db(ream_dir.clone()).expect("Unable to delete database");
            }

            let ream_db = ReamDB::new(ream_dir).expect("unable to init Ream Database");

            info!("ream database initialized ");

            initialize_db_from_checkpoint(ream_db.clone(), config.checkpoint_sync_url)
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

            let mut gossipsub_config = GossipsubConfig::default();
            gossipsub_config.set_topics(vec![GossipTopic {
                fork: network_spec().fork_digest(genesis_validators_root()),
                kind: GossipTopicKind::BeaconBlock,
            }]);

            let network_config = NetworkConfig {
                socket_address: config.socket_address,
                socket_port: config.socket_port,
                discv5_config,
                gossipsub_config,
            };

            let http_future = start_server(server_config, ream_db);

            let network_future = async {
                match Network::init(async_executor, &network_config).await {
                    Ok(mut network) => {
                        main_executor.spawn(async move {
                            network.polling_events().await;
                        });
                        tokio::signal::ctrl_c()
                            .await
                            .expect("Unable to terminate future");
                    }
                    Err(e) => {
                        error!("Failed to initialize network: {}", e);
                    }
                }
            };

            tokio::select! {
                _ = http_future => {
                    info!("HTTP server stopped!");
                },
                _ = network_future => {
                    info!("Network future completed!");
                },
            }
        }
    }
}
