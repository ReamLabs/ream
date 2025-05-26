use std::{net::IpAddr, path::PathBuf, sync::Arc};

use clap::Parser;
use ream_manager::config::ManagerConfig;
use ream_network_spec::{cli::network_parser, networks::NetworkSpec};
use ream_p2p::bootnodes::Bootnodes;
use url::Url;

use crate::cli::constants::{
    DEFAULT_DISABLE_DISCOVERY, DEFAULT_DISCOVERY_PORT, DEFAULT_HTTP_ADDRESS,
    DEFAULT_HTTP_ALLOW_ORIGIN, DEFAULT_HTTP_PORT, DEFAULT_NETWORK, DEFAULT_SOCKET_ADDRESS,
    DEFAULT_SOCKET_PORT,
};

#[derive(Debug, Parser)]
pub struct BeaconNodeConfig {
    /// Verbosity level
    #[arg(short, long, default_value_t = 3)]
    pub verbosity: u8,

    #[arg(
      long,
      help = "Choose mainnet, holesky, sepolia, hoodi, dev or provide a path to a YAML config file",
      default_value = DEFAULT_NETWORK,
      value_parser = network_parser
  )]
    pub network: Arc<NetworkSpec>,

    #[arg(long, help = "Set HTTP address", default_value_t = DEFAULT_HTTP_ADDRESS)]
    pub http_address: IpAddr,

    #[arg(long, help = "Set HTTP Port", default_value_t = DEFAULT_HTTP_PORT)]
    pub http_port: u16,

    #[arg(long, default_value_t = DEFAULT_HTTP_ALLOW_ORIGIN)]
    pub http_allow_origin: bool,

    #[arg(long, help = "Set P2P socket address", default_value_t = DEFAULT_SOCKET_ADDRESS)]
    pub socket_address: IpAddr,

    #[arg(long, help = "Set P2P socket port (TCP)", default_value_t = DEFAULT_SOCKET_PORT)]
    pub socket_port: u16,

    #[arg(long, help = "Discovery 5 listening port (UDP)", default_value_t = DEFAULT_DISCOVERY_PORT)]
    pub discovery_port: u16,

    #[arg(long, help = "Disable Discv5", default_value_t = DEFAULT_DISABLE_DISCOVERY)]
    pub disable_discovery: bool,

    #[arg(
        long,
        help = "The directory for storing application data. If used together with --ephemeral, new child directory will be created."
    )]
    pub data_dir: Option<PathBuf>,

    #[arg(
        long,
        short,
        help = "Use new data directory, located in OS temporary directory. If used together with --data-dir, new directory will be created there instead."
    )]
    pub ephemeral: bool,

    #[arg(
        default_value = "default",
        long,
        help = "One or more comma-delimited base64-encoded ENR's of peers to initially connect to. Use 'default' to use the default bootnodes for the network. Use 'none' to disable bootnodes."
    )]
    pub bootnodes: Bootnodes,

    #[arg(long, help = "Trusted RPC URL to initiate Checkpoint Sync.")]
    pub checkpoint_sync_url: Option<Url>,

    #[arg(long, help = "Purges the database.")]
    pub purge_db: bool,

    #[arg(
        long,
        help = "The URL of the execution endpoint. This is used to send requests to the engine api.",
        requires = "execution_jwt_secret"
    )]
    pub execution_endpoint: Option<Url>,

    #[arg(
        long,
        help = "The JWT secret used to authenticate with the execution endpoint. This is used to send requests to the engine api.",
        requires = "execution_endpoint"
    )]
    pub execution_jwt_secret: Option<PathBuf>,
}

impl From<BeaconNodeConfig> for ManagerConfig {
    fn from(config: BeaconNodeConfig) -> Self {
        Self {
            http_address: config.http_address,
            http_port: config.http_port,
            http_allow_origin: config.http_allow_origin,
            socket_address: config.socket_address,
            socket_port: config.socket_port,
            discovery_port: config.discovery_port,
            disable_discovery: config.disable_discovery,
            data_dir: config.data_dir,
            ephemeral: config.ephemeral,
            bootnodes: config.bootnodes,
            checkpoint_sync_url: config.checkpoint_sync_url,
            purge_db: config.purge_db,
            execution_endpoint: config.execution_endpoint,
            execution_jwt_secret: config.execution_jwt_secret,
        }
    }
}
