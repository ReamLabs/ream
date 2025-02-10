use std::net::Ipv4Addr;

use clap::Parser;
use ream::cli::{Cli, Commands};
use ream_discv5::config::NetworkConfig;
use ream_executor::ReamExecutor;
use ream_p2p::network::Network;
use tracing::info;
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

    let async_executor = ReamExecutor::new().unwrap();

    let main_executor = ReamExecutor::new().unwrap();

    let discv5_config = discv5::ConfigBuilder::new(discv5::ListenConfig::from_ip(
        Ipv4Addr::UNSPECIFIED.into(),
        8080,
    ))
    .build();
    let binding = NetworkConfig {
        discv5_config,
        boot_nodes_enr: vec![],
        disable_discovery: false,
        total_peers: 0,
    };

    match cli.command {
        Commands::Node(_cmd) => {
            info!("starting up...");
            match Network::init(async_executor, &binding).await {
                Ok(mut network) => {
                    main_executor.spawn(async move {
                        network.polling_events().await;
                    });

                    tokio::signal::ctrl_c().await.unwrap();
                }
                Err(e) => {
                    info!("Failed to initialize network: {}", e);
                    return;
                }
            }
        }
    }
}
