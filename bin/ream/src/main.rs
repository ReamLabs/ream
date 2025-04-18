use std::env;

use clap::Parser;
use ream::cli::{Cli, Commands};
use ream_discv5::{config::NetworkConfig, subnet::Subnets};
use ream_executor::ReamExecutor;
use ream_p2p::network::Network;
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

            let bootnodes = config.bootnodes.to_enrs(config.network.network);
            let binding = NetworkConfig {
                discv5_config,
                bootnodes,
                socket_address: Some(config.socket_address),
                socket_port: Some(config.socket_port),
                disable_discovery: config.disable_discovery,
                subnets: Subnets::new(),
            };

            let ream_db = ReamDB::new(config.data_dir, config.ephemeral)
                .expect("unable to init Ream Database");

            info!("ream database initialized ");

            let http_future = start_server(config.network.clone(), server_config, ream_db);

            let network_future = async {
                match Network::init(async_executor, &binding).await {
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
