use std::net::Ipv4Addr;

use clap::Parser;
use ream::cli::{Cli, Commands};
use ream_discv5::config::NetworkConfig;
use ream_executor::ReamExecutor;
use ream_p2p::{bootnodes::Bootnodes, network::Network};
use ream_storage::db::ReamDB;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    // Set the default log level to `info` if not set
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "info");
    }

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    let async_executor = ReamExecutor::new().expect("unable to create executor");

    let main_executor = ReamExecutor::new().expect("unable to create executor");

    match cli.command {
        Commands::Node(config) => {
            info!("starting up...");

            let bootnodes = Bootnodes::new(config.network.network);

            let discv5_config = discv5::ConfigBuilder::new(discv5::ListenConfig::from_ip(
                Ipv4Addr::UNSPECIFIED.into(),
                8080,
            ))
            .build();
            let binding = NetworkConfig {
                discv5_config,
                bootnodes: bootnodes.bootnodes,
                disable_discovery: false,
                total_peers: 0,
            };

            let _ream_db = ReamDB::new(config.data_dir, config.ephemeral)
                .expect("unable to init Ream Database");

            info!("ream database initialized ");

            match Network::init(async_executor, &binding).await {
                Ok(mut network) => {
                    main_executor.spawn(async move {
                        network.polling_events().await;
                    });

                    tokio::signal::ctrl_c().await.unwrap();
                }
                Err(e) => {
                    error!("Failed to initialize network: {}", e);
                    return;
                }
            }
        }
    }
}
