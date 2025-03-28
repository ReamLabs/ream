use std::{path::PathBuf, sync::Arc};

use clap::{Parser, Subcommand};
use ream_network_spec::{cli::network_parser, networks::NetworkSpec};

const DEFAULT_NETWORK: &str = "mainnet";
const DEFAULT_LISTEN_ADDRESS: &str = "0.0.0.0";
const DEFAULT_PORT: u16 = 9000;
const DEFAULT_DISCOVERY_PORT: u16 = 9000;

#[derive(Debug, Parser)]
#[command(author, version, about, long_about = None)]
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
        help = "Choose mainnet, holesky, sepolia, hoodi, or dev",
        default_value = DEFAULT_NETWORK,
        value_parser = network_parser
    )]
    pub network: Arc<NetworkSpec>,

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

    /// List of bootstrap nodes to connect to
    #[arg(long, value_delimiter = ',')]
    pub bootnodes: Vec<String>,

    /// Network listen address
    #[arg(long = "listen-address", default_value = DEFAULT_LISTEN_ADDRESS)]
    pub addr: String,

    /// Network port (TCP)
    #[arg(long = "port", default_value_t = DEFAULT_PORT)]
    pub port: u16,

    /// Discovery port (UDP)
    #[arg(long = "discovery-port", default_value_t = DEFAULT_DISCOVERY_PORT)]
    pub discovery_port: u16,
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
            "--listen-address",
            "127.0.0.1",
            "--port",
            "9001",
            "--discovery-port",
            "9002",
        ]);

        match cli.command {
            Commands::Node(config) => {
                assert_eq!(config.network.network, Network::Mainnet);
                assert_eq!(config.verbosity, 2);
                assert_eq!(config.addr, "127.0.0.1");
                assert_eq!(config.port, 9001);
                assert_eq!(config.discovery_port, 9002);
            }
        }
    }
}
