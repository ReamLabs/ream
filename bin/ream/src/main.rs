use std::env;

use clap::Parser;
use ream::cli::{Cli, Commands};
use ream_discv5::{config::DiscoveryConfig, eth2::EnrForkId, subnet::Subnets};
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
use ream_storage::db::ReamDB;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

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

            let bootnodes = config.bootnodes.to_enrs(network_spec().network);
            let discv5_config = DiscoveryConfig {
                discv5_config,
                bootnodes,
                socket_address: config.socket_address,
                socket_port: config.socket_port,
                discovery_port: config.discovery_port,
                disable_discovery: config.disable_discovery,
                subnets: Subnets::new(),
            };

            let mut gossipsub_config = GossipsubConfig::default();
            gossipsub_config.set_topics(vec![GossipTopic {
                fork: EnrForkId::electra().fork_digest,
                kind: GossipTopicKind::BeaconBlock,
            }]);

            let network_config = NetworkConfig {
                socket_address: config.socket_address,
                socket_port: config.socket_port,
                discv5_config,
                gossipsub_config,
            };

            let ream_db = ReamDB::new(config.data_dir, config.ephemeral)
                .expect("unable to init Ream Database");

            info!("ream database initialized ");

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
