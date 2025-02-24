use std::sync::Arc;

use clap::{Parser, Subcommand};
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
