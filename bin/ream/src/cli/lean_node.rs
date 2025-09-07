use std::{net::IpAddr, path::PathBuf, sync::Arc};

use clap::Parser;
use ream_network_spec::{cli::lean_network_parser, networks::LeanNetworkSpec};
use ream_p2p::bootnodes::Bootnodes;

use crate::cli::constants::{
    DEFAULT_HTTP_ADDRESS, DEFAULT_HTTP_ALLOW_ORIGIN, DEFAULT_HTTP_PORT, DEFAULT_METRICS_ADDRESS,
    DEFAULT_METRICS_ENABLED, DEFAULT_METRICS_PORT, DEFAULT_SOCKET_ADDRESS, DEFAULT_SOCKET_PORT,
};

#[derive(Debug, Parser)]
pub struct LeanNodeConfig {
    /// Verbosity level
    #[arg(short, long, default_value_t = 3)]
    pub verbosity: u8,

    #[arg(
      long,
      help = "Provide a path to a YAML config file, or use 'ephemery' for the Ephemery network",
      value_parser = lean_network_parser
  )]
    pub network: Arc<LeanNetworkSpec>,

    #[arg(
        default_value = "default",
        long,
        help = "One or more comma-delimited base64-encoded ENR's of peers to initially connect to. Use 'default' to use the default bootnodes for the network. Use 'none' to disable bootnodes."
    )]
    pub bootnodes: Bootnodes,

    #[arg(long, help = "The path to the validator registry")]
    pub validator_registry_path: PathBuf,

    #[arg(long, help = "The path to the protobuf encoded secp256k1 libp2p key")]
    pub private_key_path: Option<PathBuf>,

    #[arg(long, help = "Set P2P socket address", default_value_t = DEFAULT_SOCKET_ADDRESS)]
    pub socket_address: IpAddr,

    #[arg(long, help = "Set P2P socket port (QUIC)", default_value_t = DEFAULT_SOCKET_PORT)]
    pub socket_port: u16,

    #[arg(long, help = "Set HTTP address", default_value_t = DEFAULT_HTTP_ADDRESS)]
    pub http_address: IpAddr,

    #[arg(long, help = "Set HTTP Port", default_value_t = DEFAULT_HTTP_PORT)]
    pub http_port: u16,

    #[arg(long, default_value_t = DEFAULT_HTTP_ALLOW_ORIGIN)]
    pub http_allow_origin: bool,

    #[arg(long = "metrics", help = "Enable metrics", default_value_t = DEFAULT_METRICS_ENABLED)]
    pub enable_metrics: bool,

    #[arg(long, help = "Set metrics address", default_value_t = DEFAULT_METRICS_ADDRESS)]
    pub metrics_address: IpAddr,

    #[arg(long, help = "Set metrics port", default_value_t = DEFAULT_METRICS_PORT)]
    pub metrics_port: u16,
}
