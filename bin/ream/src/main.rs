use std::{
    net::{IpAddr, Ipv4Addr},
    str::FromStr,
};

use clap::Parser;
use discv5::Enr;
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

            let mut bootnodes = Bootnodes::new(config.network.network);
            if !config.bootnodes.is_empty() {
                let enr_bootnodes = config
                    .bootnodes
                    .iter()
                    .filter_map(|enr_str| Enr::from_str(enr_str).ok())
                    .collect::<Vec<_>>();
                bootnodes.extend(enr_bootnodes);
            }

            let discv5_config = discv5::ConfigBuilder::new(discv5::ListenConfig::from_ip(
                Ipv4Addr::from_str(&config.addr).unwrap().into(),
                config.discovery_port,
            ))
            .build();
            let binding = NetworkConfig {
                discv5_config,
                bootnodes: bootnodes.bootnodes,
                addr: IpAddr::from_str(&config.addr).unwrap(),
                port: config.port,
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
