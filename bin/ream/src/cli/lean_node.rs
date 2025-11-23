use std::{net::IpAddr, path::PathBuf};

use clap::Parser;
use ream_network_spec::{
    cli::{lean_devnet_parser, lean_network_parser},
    networks::{Devnet, LeanNetworkSpec},
};
use ream_p2p::bootnodes::Bootnodes;

use crate::cli::constants::{
    DEFAULT_DEVNET, DEFAULT_HTTP_ADDRESS, DEFAULT_HTTP_ALLOW_ORIGIN, DEFAULT_HTTP_PORT,
    DEFAULT_METRICS_ADDRESS, DEFAULT_METRICS_ENABLED, DEFAULT_METRICS_PORT, DEFAULT_SOCKET_ADDRESS,
    DEFAULT_SOCKET_PORT,
};

#[derive(Debug, Parser)]
pub struct LeanNodeConfig {
    #[arg(
      long,
      help = "Provide a path to a YAML config file, or use 'ephemery' for the Ephemery network",
      value_parser = lean_network_parser
  )]
    pub network: LeanNetworkSpec,

    #[arg(
        default_value = "default",
        long,
        help = "Bootnodes configuration: Use 'default' for network defaults, 'none' to disable, '/path/to/nodes.yaml' for a YAML file with ENRs, or comma-delimited base64-encoded ENRs"
    )]
    pub bootnodes: Bootnodes,

    #[arg(long, help = "The path to the validator registry file")]
    pub validator_registry_path: PathBuf,

    #[arg(
        default_value = "ream_0",
        long,
        help = "Node identifier for validator registry (e.g., 'ream_0', 'zeam_0')"
    )]
    pub node_id: String,

    #[arg(
        long,
        help = "The path to the hex encoded secp256k1 libp2p key",
        alias = "node-key"
    )]
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

    #[arg(long, help = "Set which devnet version to run, options are 1 and 2", default_value = DEFAULT_DEVNET, value_parser = lean_devnet_parser)]
    pub devnet: Devnet,
}
