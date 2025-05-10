use std::{
    net::{IpAddr, Ipv4Addr},
    path::PathBuf,
    sync::Arc,
};

use clap::{Parser, Subcommand};
use ream_network_spec::{cli::network_parser, networks::NetworkSpec};
use ream_node::version::FULL_VERSION;
use ream_p2p::bootnodes::Bootnodes;
use reqwest::Url;

const DEFAULT_DISABLE_DISCOVERY: bool = false;
const DEFAULT_DISCOVERY_PORT: u16 = 9000;
const DEFAULT_HTTP_ADDRESS: IpAddr = IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1));
const DEFAULT_HTTP_ALLOW_ORIGIN: bool = false;
const DEFAULT_HTTP_PORT: u16 = 5052;
const DEFAULT_NETWORK: &str = "mainnet";
const DEFAULT_SOCKET_ADDRESS: IpAddr = IpAddr::V4(Ipv4Addr::UNSPECIFIED);
const DEFAULT_SOCKET_PORT: u16 = 9000;

#[derive(Debug, Parser)]
#[command(author, version = FULL_VERSION, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Start the node
    #[command(name = "node")]
    Node(NodeConfig),
}

#[derive(Debug, Parser)]
pub struct NodeConfig {
    /// Verbosity level
    #[arg(short, long, default_value_t = 3)]
    pub verbosity: u8,

    #[arg(
        long,
        help = "Choose mainnet, holesky, sepolia, hoodi or dev",
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
        long = "bootnodes",
        help = "One or more comma-delimited base64-encoded ENR's of peers to initially connect to. Use 'default' to use the default bootnodes for the network. Use 'none' to disable bootnodes."
    )]
    pub bootnodes: Bootnodes,

    #[arg(
        long = "checkpoint-sync-url",
        help = "Trusted RPC URL to initiate Checkpoint Sync."
    )]
    pub checkpoint_sync_url: Option<Url>,

    #[arg(long = "purge-db", help = "Purges the database.")]
    pub purge_db: bool,
}

#[cfg(test)]
mod tests {
    use ream_network_spec::networks::Network;

    use super::*;

    #[test]
    fn test_cli_node_command() {
        let cli = Cli::parse_from([
            "program",
            "node",
            "--verbosity",
            "2",
            "--socket-address",
            "127.0.0.1",
            "--socket-port",
            "9001",
            "--discovery-port",
            "9002",
        ]);

        match cli.command {
            Commands::Node(config) => {
                assert_eq!(config.network.network, Network::Mainnet);
                assert_eq!(config.verbosity, 2);
                assert_eq!(
                    config.socket_address,
                    IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1))
                );
                assert_eq!(config.socket_port, 9001);
                assert_eq!(config.discovery_port, 9002);
            }
        }
    }
}
