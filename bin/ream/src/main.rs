use std::{env, net::Ipv4Addr};

use clap::Parser;
use ream::cli::{Cli, Commands};
use ream_discv5::config::NetworkConfig;
use ream_executor::ReamExecutor;
use ream_p2p::{bootnodes::Bootnodes, network::Network};
use ream_storage::db::ReamDB;
use ream_rpc::{config::ServerConfig, start_server, utils::chain::BeaconChain};
use std::sync::Arc;
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

            let bootnodes = Bootnodes::new(config.network.network);

            let beacon_chain = Arc::new(BeaconChain::mock_init());
            let server_config =
                ServerConfig::from_args(_cmd.http_port, _cmd.http_address, _cmd.http_allow_origin);
            println!("check");

            let http_future = start_server(beacon_chain, server_config);

                    let discv5_config = discv5::ConfigBuilder::new(discv5::ListenConfig::from_ip(
                        Ipv4Addr::UNSPECIFIED.into(),
                        _cmd.discv_listen_port,
                    ))
                    .build();
                    let binding = NetworkConfig {
                        discv5_config,
                        bootnodes: bootnodes.bootnodes,
                        disable_discovery: _cmd.disable_discovery,
                        total_peers: 0,
                    };

            let _ream_db = ReamDB::new(config.data_dir, config.ephemeral)
                .expect("unable to init Ream Database");

            info!("ream database initialized ");

            let network=async{match Network::init(async_executor, &binding).await {
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
            };

            tokio::select! {
                _ = http_future => {},
                _ = network_future => {},
            }
            tokio::signal::ctrl_c().await.unwrap();
        }
    }
}