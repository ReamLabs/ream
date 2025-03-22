use std::sync::Arc;

use clap::{ArgAction, Parser, Subcommand};
use ream_network_spec::{cli::network_parser, networks::NetworkSpec};

const DEFAULT_NETWORK: &str = "mainnet";

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
    Node(NodeCommand),
}

#[derive(Debug, Parser)]
pub struct NodeCommand {
    /// Verbosity level
    #[arg(short, long, default_value_t = 3)]
    pub verbosity: u8,

    #[arg(
        long,
        help = "Choose mainnet, holesky, or sepolia",
        default_value = DEFAULT_NETWORK,
        value_parser = network_parser
    )]
    pub network: Arc<NetworkSpec>,

    /// HTTP port number
    #[arg(long, default_value_t = 5052)]
    pub http_port: usize,

    /// HTTP bind address
    #[arg(long,default_value_t = String::from("127.0.0.1"))]
    pub http_address: String,

    /// Allow CORS
    #[arg(long, action = ArgAction::SetTrue)]
    pub http_allow_origin: bool,

    /// discv5 listening port
    #[arg(long, default_value_t = 8080)]
    pub discv_listen_port: u16,

    /// disable discovery
    #[arg(long, action = ArgAction::SetTrue)]
    pub disable_discovery: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_node_command() {
        let cli = Cli::parse_from(["program", "node", "--verbosity", "2"]);

        match cli.command {
            Commands::Node(cmd) => {
                assert_eq!(cmd.verbosity, 2);
            }
        }
    }
}
